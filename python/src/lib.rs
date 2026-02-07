use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use mnemo_core::embedding::openai::OpenAiEmbedding;
use mnemo_core::embedding::{EmbeddingProvider, NoopEmbedding};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::index::VectorIndex;
use mnemo_core::model::memory::{MemoryType, Scope};
use mnemo_core::query::branch::BranchRequest;
use mnemo_core::query::checkpoint::CheckpointRequest;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::merge::MergeRequest;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::replay::ReplayRequest;
use mnemo_core::query::share::ShareRequest;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn to_py_err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

#[pyclass]
struct MnemoClient {
    engine: Arc<MnemoEngine>,
    index: Arc<UsearchIndex>,
    runtime: tokio::runtime::Runtime,
    index_path: std::path::PathBuf,
}

#[pymethods]
impl MnemoClient {
    #[new]
    #[pyo3(signature = (db_path="mnemo.db", agent_id="default", org_id=None, openai_api_key=None, embedding_model="text-embedding-3-small", dimensions=1536))]
    fn new(
        db_path: &str,
        agent_id: &str,
        org_id: Option<String>,
        openai_api_key: Option<String>,
        embedding_model: &str,
        dimensions: usize,
    ) -> PyResult<Self> {
        let runtime = tokio::runtime::Runtime::new().map_err(to_py_err)?;
        let db_path = std::path::PathBuf::from(db_path);
        let storage = Arc::new(DuckDbStorage::open(&db_path).map_err(to_py_err)?);

        let embedding: Arc<dyn EmbeddingProvider> = if let Some(api_key) = openai_api_key {
            Arc::new(OpenAiEmbedding::new(
                api_key,
                embedding_model.to_string(),
                dimensions,
            ))
        } else {
            Arc::new(NoopEmbedding::new(dimensions))
        };

        let index = Arc::new(UsearchIndex::new(dimensions).map_err(to_py_err)?);

        let index_path = db_path.with_extension("usearch");
        if index_path.exists() {
            index.load(&index_path).map_err(to_py_err)?;
        }

        let engine = Arc::new(MnemoEngine::new(
            storage,
            index.clone(),
            embedding,
            agent_id.to_string(),
            org_id,
        ));

        Ok(Self {
            engine,
            index,
            runtime,
            index_path,
        })
    }

    #[pyo3(signature = (content, memory_type=None, scope=None, importance=None, tags=None, metadata=None, thread_id=None, ttl_seconds=None, related_to=None))]
    #[allow(clippy::too_many_arguments)]
    fn remember(
        &self,
        content: String,
        memory_type: Option<String>,
        scope: Option<String>,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        metadata: Option<&Bound<'_, PyDict>>,
        thread_id: Option<String>,
        ttl_seconds: Option<u64>,
        related_to: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        let metadata_value = match metadata {
            Some(dict) => pythonize_dict(dict)?,
            None => None,
        };

        let request = RememberRequest {
            content,
            agent_id: None,
            memory_type: memory_type.and_then(|s| s.parse::<MemoryType>().ok()),
            scope: scope.and_then(|s| s.parse::<Scope>().ok()),
            importance,
            tags,
            metadata: metadata_value,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id,
            ttl_seconds,
            related_to,
            decay_rate: None,
            created_by: None,
        };

        let response = self
            .runtime
            .block_on(self.engine.remember(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("id", response.id.to_string())?;
            dict.set_item("content_hash", response.content_hash)?;
            Ok(dict.into())
        })
    }

    /// Mem0-compatible alias for remember
    #[pyo3(signature = (content, memory_type=None, scope=None, importance=None, tags=None, metadata=None))]
    fn add(
        &self,
        content: String,
        memory_type: Option<String>,
        scope: Option<String>,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        metadata: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        self.remember(content, memory_type, scope, importance, tags, metadata, None, None, None)
    }

    #[pyo3(signature = (query, limit=None, memory_type=None, min_importance=None, tags=None, strategy=None))]
    fn recall(
        &self,
        query: String,
        limit: Option<usize>,
        memory_type: Option<String>,
        min_importance: Option<f32>,
        tags: Option<Vec<String>>,
        strategy: Option<String>,
    ) -> PyResult<PyObject> {
        let request = RecallRequest {
            query,
            agent_id: None,
            limit,
            memory_type: memory_type.and_then(|s| s.parse::<MemoryType>().ok()),
            memory_types: None,
            scope: None,
            min_importance,
            tags,
            org_id: None,
            strategy,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        };

        let response = self
            .runtime
            .block_on(self.engine.recall(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let result = PyDict::new(py);
            let memories: Vec<PyObject> = response
                .memories
                .iter()
                .map(|m| {
                    let dict = PyDict::new(py);
                    dict.set_item("id", m.id.to_string()).unwrap();
                    dict.set_item("content", &m.content).unwrap();
                    dict.set_item("agent_id", &m.agent_id).unwrap();
                    dict.set_item("memory_type", m.memory_type.to_string())
                        .unwrap();
                    dict.set_item("scope", m.scope.to_string()).unwrap();
                    dict.set_item("importance", m.importance).unwrap();
                    dict.set_item("tags", &m.tags).unwrap();
                    dict.set_item("score", m.score).unwrap();
                    dict.set_item("access_count", m.access_count).unwrap();
                    dict.set_item("created_at", &m.created_at).unwrap();
                    dict.set_item("updated_at", &m.updated_at).unwrap();
                    dict.into_any().unbind()
                })
                .collect();
            result.set_item("memories", memories)?;
            result.set_item("total", response.total)?;
            Ok(result.into())
        })
    }

    /// Mem0-compatible alias for recall
    #[pyo3(signature = (query, limit=None, memory_type=None, min_importance=None, tags=None))]
    fn search(
        &self,
        query: String,
        limit: Option<usize>,
        memory_type: Option<String>,
        min_importance: Option<f32>,
        tags: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        self.recall(query, limit, memory_type, min_importance, tags, None)
    }

    #[pyo3(signature = (memory_ids, strategy=None))]
    fn forget(&self, memory_ids: Vec<String>, strategy: Option<String>) -> PyResult<PyObject> {
        let parsed_ids: Result<Vec<uuid::Uuid>, _> = memory_ids
            .iter()
            .map(|s| uuid::Uuid::parse_str(s))
            .collect();
        let parsed_ids = parsed_ids.map_err(to_py_err)?;

        let request = ForgetRequest {
            memory_ids: parsed_ids,
            agent_id: None,
            strategy: strategy.map(|s| match s.as_str() {
                "hard_delete" => ForgetStrategy::HardDelete,
                "decay" => ForgetStrategy::Decay,
                "consolidate" => ForgetStrategy::Consolidate,
                "archive" => ForgetStrategy::Archive,
                _ => ForgetStrategy::SoftDelete,
            }),
            criteria: None,
        };

        let response = self
            .runtime
            .block_on(self.engine.forget(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            let forgotten: Vec<String> = response.forgotten.iter().map(|id| id.to_string()).collect();
            dict.set_item("forgotten", forgotten)?;
            dict.set_item(
                "errors",
                response
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.id, e.error))
                    .collect::<Vec<_>>(),
            )?;
            Ok(dict.into())
        })
    }

    /// Mem0-compatible alias for forget
    #[pyo3(signature = (memory_ids, strategy=None))]
    fn delete(&self, memory_ids: Vec<String>, strategy: Option<String>) -> PyResult<PyObject> {
        self.forget(memory_ids, strategy)
    }

    #[pyo3(signature = (memory_id, target_agent_id, permission=None))]
    fn share(
        &self,
        memory_id: String,
        target_agent_id: String,
        permission: Option<String>,
    ) -> PyResult<PyObject> {
        let mid = uuid::Uuid::parse_str(&memory_id).map_err(to_py_err)?;

        let request = ShareRequest {
            memory_id: mid,
            agent_id: None,
            target_agent_id,
            target_agent_ids: None,
            permission: permission.and_then(|s| s.parse().ok()),
            expires_in_hours: None,
        };

        let response = self
            .runtime
            .block_on(self.engine.share(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("acl_id", response.acl_id.to_string())?;
            dict.set_item("memory_id", response.memory_id.to_string())?;
            dict.set_item("shared_with", &response.shared_with)?;
            dict.set_item("permission", response.permission.to_string())?;
            Ok(dict.into())
        })
    }

    #[pyo3(signature = (thread_id, state_snapshot, branch_name=None, label=None, metadata=None))]
    fn checkpoint(
        &self,
        thread_id: String,
        state_snapshot: &Bound<'_, PyDict>,
        branch_name: Option<String>,
        label: Option<String>,
        metadata: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let snapshot_value = pythonize_dict(state_snapshot)?.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let metadata_value = match metadata {
            Some(dict) => pythonize_dict(dict)?,
            None => None,
        };

        let request = CheckpointRequest {
            thread_id,
            agent_id: None,
            branch_name,
            state_snapshot: snapshot_value,
            label,
            metadata: metadata_value,
        };

        let response = self
            .runtime
            .block_on(self.engine.checkpoint(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("checkpoint_id", response.id.to_string())?;
            dict.set_item("parent_id", response.parent_id.map(|id| id.to_string()))?;
            dict.set_item("branch_name", &response.branch_name)?;
            Ok(dict.into())
        })
    }

    #[pyo3(signature = (thread_id, new_branch_name, source_checkpoint_id=None, source_branch=None))]
    fn branch(
        &self,
        thread_id: String,
        new_branch_name: String,
        source_checkpoint_id: Option<String>,
        source_branch: Option<String>,
    ) -> PyResult<PyObject> {
        let request = BranchRequest {
            thread_id,
            agent_id: None,
            new_branch_name,
            source_checkpoint_id: source_checkpoint_id.and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            source_branch,
        };

        let response = self
            .runtime
            .block_on(self.engine.branch(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("checkpoint_id", response.checkpoint_id.to_string())?;
            dict.set_item("branch_name", &response.branch_name)?;
            dict.set_item("source_checkpoint_id", response.source_checkpoint_id.to_string())?;
            Ok(dict.into())
        })
    }

    #[pyo3(signature = (thread_id, source_branch, target_branch=None, strategy=None, cherry_pick_ids=None))]
    fn merge(
        &self,
        thread_id: String,
        source_branch: String,
        target_branch: Option<String>,
        strategy: Option<String>,
        cherry_pick_ids: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        use mnemo_core::query::merge::MergeStrategy;

        let merge_strategy = strategy.as_deref().map(|s| match s {
            "cherry_pick" => MergeStrategy::CherryPick,
            "squash" => MergeStrategy::Squash,
            _ => MergeStrategy::FullMerge,
        });

        let cherry_ids = cherry_pick_ids.map(|ids| {
            ids.iter()
                .filter_map(|s| uuid::Uuid::parse_str(s).ok())
                .collect()
        });

        let request = MergeRequest {
            thread_id,
            agent_id: None,
            source_branch,
            target_branch,
            strategy: merge_strategy,
            cherry_pick_ids: cherry_ids,
        };

        let response = self
            .runtime
            .block_on(self.engine.merge(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("checkpoint_id", response.checkpoint_id.to_string())?;
            dict.set_item("target_branch", &response.target_branch)?;
            dict.set_item("merged_memory_count", response.merged_memory_count)?;
            Ok(dict.into())
        })
    }

    #[pyo3(signature = (thread_id, checkpoint_id=None, branch_name=None))]
    fn replay(
        &self,
        thread_id: String,
        checkpoint_id: Option<String>,
        branch_name: Option<String>,
    ) -> PyResult<PyObject> {
        let request = ReplayRequest {
            thread_id,
            agent_id: None,
            checkpoint_id: checkpoint_id.and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            branch_name,
        };

        let response = self
            .runtime
            .block_on(self.engine.replay(request))
            .map_err(to_py_err)?;

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            let cp_dict = PyDict::new(py);
            cp_dict.set_item("id", response.checkpoint.id.to_string())?;
            cp_dict.set_item("branch_name", &response.checkpoint.branch_name)?;
            cp_dict.set_item("label", &response.checkpoint.label)?;
            cp_dict.set_item("created_at", &response.checkpoint.created_at)?;
            dict.set_item("checkpoint", cp_dict)?;
            dict.set_item("memory_count", response.memories.len())?;
            dict.set_item("event_count", response.events.len())?;

            let memories: Vec<PyObject> = response.memories.iter().map(|m| {
                let d = PyDict::new(py);
                d.set_item("id", m.id.to_string()).unwrap();
                d.set_item("content", &m.content).unwrap();
                d.set_item("memory_type", m.memory_type.to_string()).unwrap();
                d.into_any().unbind()
            }).collect();
            dict.set_item("memories", memories)?;

            Ok(dict.into())
        })
    }

    fn save_index(&self) -> PyResult<()> {
        self.index.save(&self.index_path).map_err(to_py_err)
    }

    fn index_size(&self) -> usize {
        self.index.len()
    }
}

impl Drop for MnemoClient {
    fn drop(&mut self) {
        let _ = self.index.save(&self.index_path);
    }
}

fn pythonize_dict(dict: &Bound<'_, PyDict>) -> PyResult<Option<serde_json::Value>> {
    let py = dict.py();
    let json_mod = py.import("json")?;
    let json_str: String = json_mod
        .call_method1("dumps", (dict,))?
        .extract()?;
    let value: serde_json::Value = serde_json::from_str(&json_str).map_err(to_py_err)?;
    Ok(Some(value))
}

#[pymodule]
fn _mnemo(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MnemoClient>()?;
    Ok(())
}
