pub mod tantivy_index;

use crate::error::Result;
use uuid::Uuid;

pub trait FullTextIndex: Send + Sync {
    fn add(&self, id: Uuid, content: &str) -> Result<()>;
    fn remove(&self, id: Uuid) -> Result<()>;
    fn search(&self, query: &str, limit: usize) -> Result<Vec<(Uuid, f32)>>;
    fn commit(&self) -> Result<()>;
    fn save(&self) -> Result<()>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
