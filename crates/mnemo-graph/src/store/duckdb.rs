//! DuckDB-backed [`GraphStore`].
//!
//! Tables match the v0.4.0-rc1 migration shape — one row per edge,
//! `valid_to` stored as nullable RFC3339 string. We inherit the
//! `Arc<Mutex<duckdb::Connection>>` + `spawn_blocking` pattern from
//! `mnemo-core::storage::duckdb` so the trait methods stay async-safe
//! despite DuckDB's not-Send connection.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use duckdb::Connection;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::model::TemporalEdge;
use crate::store::{Error, GraphStore, Result};

/// SQL — kept as one string each so a future migrator can diff them
/// against a stored schema version.
pub const CREATE_GRAPH_NODES_TABLE: &str = "
CREATE TABLE IF NOT EXISTS graph_nodes (
    id VARCHAR PRIMARY KEY,
    label VARCHAR,
    metadata JSON,
    created_at VARCHAR NOT NULL
);
";

pub const CREATE_GRAPH_EDGES_TABLE: &str = "
CREATE TABLE IF NOT EXISTS graph_edges (
    id VARCHAR PRIMARY KEY,
    src VARCHAR NOT NULL,
    dst VARCHAR NOT NULL,
    relation VARCHAR NOT NULL,
    valid_from VARCHAR NOT NULL,
    valid_to VARCHAR,
    confidence FLOAT NOT NULL DEFAULT 1.0,
    recorded_at VARCHAR NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_graph_edges_src_validfrom
    ON graph_edges(src, valid_from);
CREATE INDEX IF NOT EXISTS idx_graph_edges_dst
    ON graph_edges(dst);
";

pub struct DuckGraphStore {
    conn: Arc<Mutex<Connection>>,
}

impl DuckGraphStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(CREATE_GRAPH_NODES_TABLE)?;
    conn.execute_batch(CREATE_GRAPH_EDGES_TABLE)?;
    Ok(())
}

#[async_trait]
impl GraphStore for DuckGraphStore {
    async fn insert_edge(&self, edge: &TemporalEdge) -> Result<()> {
        let conn = self.conn.lock().await;
        let valid_to_s: Option<String> = edge.valid_to.map(|v| v.to_rfc3339());
        // UPSERT — DuckDB supports ON CONFLICT for primary keys.
        conn.execute(
            "INSERT OR REPLACE INTO graph_edges
             (id, src, dst, relation, valid_from, valid_to, confidence, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                edge.id.to_string(),
                edge.src.to_string(),
                edge.dst.to_string(),
                edge.relation,
                edge.valid_from.to_rfc3339(),
                valid_to_s,
                edge.confidence,
                edge.recorded_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn close_edge(&self, edge_id: Uuid, closed_at: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().await;
        // Only update rows whose valid_to is currently NULL — closing
        // an already-closed edge is a no-op.
        conn.execute(
            "UPDATE graph_edges SET valid_to = ?
             WHERE id = ? AND valid_to IS NULL",
            duckdb::params![closed_at.to_rfc3339(), edge_id.to_string()],
        )?;
        Ok(())
    }

    async fn outgoing_at(&self, node: Uuid, as_of: DateTime<Utc>) -> Result<Vec<TemporalEdge>> {
        let conn = self.conn.lock().await;
        let as_of_s = as_of.to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, valid_from, valid_to, confidence, recorded_at
             FROM graph_edges
             WHERE src = ?
               AND valid_from <= ?
               AND (valid_to IS NULL OR valid_to > ?)
             ORDER BY confidence DESC, recorded_at DESC",
        )?;
        let rows = stmt.query_map(
            duckdb::params![node.to_string(), as_of_s.clone(), as_of_s],
            row_to_edge,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    async fn all_edges(&self) -> Result<Vec<TemporalEdge>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, valid_from, valid_to, confidence, recorded_at
             FROM graph_edges
             ORDER BY recorded_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_edge)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn row_to_edge(row: &duckdb::Row<'_>) -> std::result::Result<TemporalEdge, ::duckdb::Error> {
    let id: String = row.get(0)?;
    let src: String = row.get(1)?;
    let dst: String = row.get(2)?;
    let relation: String = row.get(3)?;
    let valid_from: String = row.get(4)?;
    let valid_to: Option<String> = row.get(5)?;
    let confidence: f32 = row.get(6)?;
    let recorded_at: String = row.get(7)?;
    let parse = |s: &str| -> std::result::Result<DateTime<Utc>, ::duckdb::Error> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                ::duckdb::Error::FromSqlConversionFailure(
                    0,
                    ::duckdb::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    Ok(TemporalEdge {
        id: parse_uuid(&id)?,
        src: parse_uuid(&src)?,
        dst: parse_uuid(&dst)?,
        relation,
        valid_from: parse(&valid_from)?,
        valid_to: valid_to.as_deref().map(parse).transpose()?,
        confidence,
        recorded_at: parse(&recorded_at)?,
    })
}

fn parse_uuid(s: &str) -> std::result::Result<Uuid, ::duckdb::Error> {
    Uuid::parse_str(s).map_err(|e| {
        ::duckdb::Error::FromSqlConversionFailure(0, ::duckdb::types::Type::Text, Box::new(e))
    })
}

// Drop hint: keep `Error` import warm so `cargo check` doesn't elide it.
#[allow(dead_code)]
fn _ensure_error_used(e: ::duckdb::Error) -> Error {
    Error::from(e)
}
