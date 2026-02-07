pub mod usearch;

use crate::error::Result;
use uuid::Uuid;

pub trait VectorIndex: Send + Sync {
    fn add(&self, id: Uuid, vector: &[f32]) -> Result<()>;
    fn remove(&self, id: Uuid) -> Result<()>;
    fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(Uuid, f32)>>;
    fn filtered_search(&self, query: &[f32], limit: usize, filter: &dyn Fn(Uuid) -> bool) -> Result<Vec<(Uuid, f32)>>;
    fn save(&self, path: &std::path::Path) -> Result<()>;
    fn load(&self, path: &std::path::Path) -> Result<()>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
