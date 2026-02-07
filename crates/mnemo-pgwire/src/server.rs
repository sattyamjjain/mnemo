//! PostgreSQL wire protocol connection handler.
//!
//! Implements the subset of the PostgreSQL wire protocol needed for
//! simple query execution. Handles startup, authentication (trust mode),
//! and the simple query flow.
//!
//! Reference: <https://www.postgresql.org/docs/current/protocol.html>

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use mnemo_core::query::MnemoEngine;

use crate::parser::{self, ParsedStatement};
use crate::PgWireConfig;

/// Handle a single PostgreSQL wire protocol connection.
pub async fn handle_connection(
    mut stream: TcpStream,
    engine: Arc<MnemoEngine>,
    config: &PgWireConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Phase 1: Startup message
    let startup_len = stream.read_i32().await? as usize;
    if startup_len < 8 || startup_len > 10240 {
        return Err("invalid startup message length".into());
    }

    let mut startup_buf = vec![0u8; startup_len - 4];
    stream.read_exact(&mut startup_buf).await?;

    let protocol_version = i32::from_be_bytes([
        startup_buf[0],
        startup_buf[1],
        startup_buf[2],
        startup_buf[3],
    ]);

    // SSL request (80877103) — respond with 'N' (no SSL)
    if protocol_version == 80877103 {
        stream.write_all(b"N").await?;
        // Client will retry with normal startup
        let startup_len = stream.read_i32().await? as usize;
        if startup_len < 8 || startup_len > 10240 {
            return Err("invalid startup message length after SSL".into());
        }
        startup_buf = vec![0u8; startup_len - 4];
        stream.read_exact(&mut startup_buf).await?;
    }

    // Phase 2: Send AuthenticationOk
    // Type 'R' (Authentication), length 8, auth type 0 (OK/trust)
    stream
        .write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 0])
        .await?;

    // Send ParameterStatus messages
    send_parameter_status(&mut stream, "server_version", "16.0").await?;
    send_parameter_status(&mut stream, "server_encoding", "UTF8").await?;
    send_parameter_status(&mut stream, "client_encoding", "UTF8").await?;
    send_parameter_status(&mut stream, "application_name", "mnemo-pgwire").await?;

    // Send ReadyForQuery
    send_ready_for_query(&mut stream).await?;

    // Phase 3: Query loop
    loop {
        let msg_type = match stream.read_u8().await {
            Ok(b) => b,
            Err(_) => break, // Connection closed
        };

        let msg_len = stream.read_i32().await? as usize;
        if msg_len < 4 || msg_len > 1_048_576 {
            break;
        }

        let mut msg_buf = vec![0u8; msg_len - 4];
        if !msg_buf.is_empty() {
            stream.read_exact(&mut msg_buf).await?;
        }

        match msg_type {
            b'Q' => {
                // Simple Query
                let sql = String::from_utf8_lossy(&msg_buf)
                    .trim_end_matches('\0')
                    .to_string();

                tracing::debug!("pgwire query: {sql}");

                match handle_query(&sql, &engine, config).await {
                    Ok(response) => {
                        send_query_response(&mut stream, &response).await?;
                    }
                    Err(e) => {
                        send_error(&mut stream, &e.to_string()).await?;
                    }
                }

                send_ready_for_query(&mut stream).await?;
            }
            b'X' => {
                // Terminate
                tracing::debug!("pgwire client terminated");
                break;
            }
            _ => {
                // Unsupported message type — send error and continue
                send_error(
                    &mut stream,
                    &format!("unsupported message type: {}", msg_type as char),
                )
                .await?;
                send_ready_for_query(&mut stream).await?;
            }
        }
    }

    Ok(())
}

/// Query response rows.
struct QueryResponse {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    command_tag: String,
}

async fn handle_query(
    sql: &str,
    engine: &MnemoEngine,
    config: &PgWireConfig,
) -> Result<QueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    let stmt = parser::parse_sql(sql);

    match stmt {
        ParsedStatement::Select(q) => {
            let agent_id = q
                .agent_id
                .unwrap_or_else(|| config.default_agent_id.clone());

            let request = mnemo_core::query::recall::RecallRequest {
                agent_id: Some(agent_id),
                query: q.query_text.unwrap_or_default(),
                limit: Some(q.limit),
                memory_type: None,
                memory_types: None,
                scope: None,
                strategy: Some("exact".to_string()),
                min_importance: None,
                tags: None,
                org_id: None,
                temporal_range: None,
                recency_half_life_hours: None,
                hybrid_weights: None,
                rrf_k: None,
                as_of: None,
            };

            let response = engine.recall(request).await?;

            let columns = vec![
                "id".to_string(),
                "agent_id".to_string(),
                "content".to_string(),
                "memory_type".to_string(),
                "importance".to_string(),
                "created_at".to_string(),
            ];

            let rows: Vec<Vec<String>> = response
                .memories
                .iter()
                .skip(q.offset)
                .map(|m| {
                    vec![
                        m.id.to_string(),
                        m.agent_id.clone(),
                        m.content.clone(),
                        m.memory_type.to_string(),
                        m.importance.to_string(),
                        m.created_at.clone(),
                    ]
                })
                .collect();

            let count = rows.len();
            Ok(QueryResponse {
                columns,
                rows,
                command_tag: format!("SELECT {count}"),
            })
        }

        ParsedStatement::Insert(q) => {
            let agent_id = q
                .agent_id
                .unwrap_or_else(|| config.default_agent_id.clone());

            let request = mnemo_core::query::remember::RememberRequest {
                content: q.content,
                agent_id: Some(agent_id),
                memory_type: q
                    .memory_type
                    .as_deref()
                    .and_then(parse_memory_type),
                scope: None,
                importance: q.importance,
                tags: if q.tags.is_empty() { None } else { Some(q.tags) },
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };

            let response = engine.remember(request).await?;

            Ok(QueryResponse {
                columns: vec!["id".to_string(), "content_hash".to_string()],
                rows: vec![vec![
                    response.id.to_string(),
                    response.content_hash.clone(),
                ]],
                command_tag: "INSERT 0 1".to_string(),
            })
        }

        ParsedStatement::Delete(q) => {
            if let Some(memory_id_str) = q.memory_id {
                let memory_id: uuid::Uuid = memory_id_str.parse().map_err(|e| {
                    format!("invalid UUID in DELETE WHERE id = '...': {e}")
                })?;

                let agent_id = q
                    .agent_id
                    .unwrap_or_else(|| config.default_agent_id.clone());

                let request = mnemo_core::query::forget::ForgetRequest {
                    memory_ids: vec![memory_id],
                    agent_id: Some(agent_id),
                    strategy: Some(mnemo_core::query::forget::ForgetStrategy::SoftDelete),
                    criteria: None,
                };

                let response = engine.forget(request).await?;
                let count = response.forgotten.len();

                Ok(QueryResponse {
                    columns: vec![],
                    rows: vec![],
                    command_tag: format!("DELETE {count}"),
                })
            } else {
                Err("DELETE requires WHERE id = '...' clause".into())
            }
        }

        ParsedStatement::Unsupported(s) => Err(format!("unsupported SQL: {s}").into()),
    }
}

fn parse_memory_type(s: &str) -> Option<mnemo_core::model::memory::MemoryType> {
    match s.to_lowercase().as_str() {
        "episodic" => Some(mnemo_core::model::memory::MemoryType::Episodic),
        "semantic" => Some(mnemo_core::model::memory::MemoryType::Semantic),
        "procedural" => Some(mnemo_core::model::memory::MemoryType::Procedural),
        "working" => Some(mnemo_core::model::memory::MemoryType::Working),
        _ => None,
    }
}

async fn send_parameter_status(
    stream: &mut TcpStream,
    name: &str,
    value: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = Vec::new();
    buf.push(b'S'); // ParameterStatus type

    let name_bytes = name.as_bytes();
    let value_bytes = value.as_bytes();
    let len = 4 + name_bytes.len() + 1 + value_bytes.len() + 1;
    buf.extend_from_slice(&(len as i32).to_be_bytes());
    buf.extend_from_slice(name_bytes);
    buf.push(0);
    buf.extend_from_slice(value_bytes);
    buf.push(0);

    stream.write_all(&buf).await?;
    Ok(())
}

async fn send_ready_for_query(
    stream: &mut TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // ReadyForQuery: type 'Z', length 5, transaction status 'I' (idle)
    stream.write_all(&[b'Z', 0, 0, 0, 5, b'I']).await?;
    Ok(())
}

async fn send_error(
    stream: &mut TcpStream,
    message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = Vec::new();
    buf.push(b'E'); // ErrorResponse type

    let mut fields = Vec::new();
    // Severity
    fields.push(b'S');
    fields.extend_from_slice(b"ERROR\0");
    // SQLSTATE (42000 = syntax error)
    fields.push(b'C');
    fields.extend_from_slice(b"42000\0");
    // Message
    fields.push(b'M');
    fields.extend_from_slice(message.as_bytes());
    fields.push(0);
    // Terminator
    fields.push(0);

    let len = 4 + fields.len();
    buf.extend_from_slice(&(len as i32).to_be_bytes());
    buf.extend_from_slice(&fields);

    stream.write_all(&buf).await?;
    Ok(())
}

async fn send_query_response(
    stream: &mut TcpStream,
    response: &QueryResponse,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !response.columns.is_empty() {
        // RowDescription
        let mut desc_buf = Vec::new();
        desc_buf.extend_from_slice(&(response.columns.len() as i16).to_be_bytes());

        for col in &response.columns {
            desc_buf.extend_from_slice(col.as_bytes());
            desc_buf.push(0); // null terminator
            desc_buf.extend_from_slice(&0i32.to_be_bytes()); // table OID
            desc_buf.extend_from_slice(&0i16.to_be_bytes()); // column attr number
            desc_buf.extend_from_slice(&25i32.to_be_bytes()); // type OID (text = 25)
            desc_buf.extend_from_slice(&(-1i16).to_be_bytes()); // type size (-1 = variable)
            desc_buf.extend_from_slice(&(-1i32).to_be_bytes()); // type modifier
            desc_buf.extend_from_slice(&0i16.to_be_bytes()); // format code (text = 0)
        }

        let mut msg = Vec::new();
        msg.push(b'T'); // RowDescription type
        let len = 4 + desc_buf.len();
        msg.extend_from_slice(&(len as i32).to_be_bytes());
        msg.extend_from_slice(&desc_buf);
        stream.write_all(&msg).await?;

        // DataRow for each row
        for row in &response.rows {
            let mut row_buf = Vec::new();
            row_buf.extend_from_slice(&(row.len() as i16).to_be_bytes());

            for val in row {
                let bytes = val.as_bytes();
                row_buf.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                row_buf.extend_from_slice(bytes);
            }

            let mut msg = Vec::new();
            msg.push(b'D'); // DataRow type
            let len = 4 + row_buf.len();
            msg.extend_from_slice(&(len as i32).to_be_bytes());
            msg.extend_from_slice(&row_buf);
            stream.write_all(&msg).await?;
        }
    }

    // CommandComplete
    let tag = response.command_tag.as_bytes();
    let mut msg = Vec::new();
    msg.push(b'C'); // CommandComplete type
    let len = 4 + tag.len() + 1;
    msg.extend_from_slice(&(len as i32).to_be_bytes());
    msg.extend_from_slice(tag);
    msg.push(0);
    stream.write_all(&msg).await?;

    Ok(())
}
