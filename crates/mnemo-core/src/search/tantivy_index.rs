use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, STORED, STRING, TEXT};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};
use tantivy::schema::Value;

use crate::error::{Error, Result};
use crate::search::FullTextIndex;
use uuid::Uuid;

pub struct TantivyFullTextIndex {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    id_field: tantivy::schema::Field,
    content_field: tantivy::schema::Field,
}

fn build_schema() -> (Schema, tantivy::schema::Field, tantivy::schema::Field) {
    let mut schema_builder = Schema::builder();
    let id_field = schema_builder.add_text_field("id", STRING | STORED);
    let content_field = schema_builder.add_text_field("content", TEXT);
    (schema_builder.build(), id_field, content_field)
}

impl TantivyFullTextIndex {
    pub fn new(path: &Path) -> Result<Self> {
        let (schema, id_field, content_field) = build_schema();

        std::fs::create_dir_all(path).map_err(|e| Error::Index(e.to_string()))?;

        let dir = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| Error::Index(e.to_string()))?;

        let index = if Index::exists(&dir).map_err(|e| Error::Index(e.to_string()))? {
            Index::open(dir).map_err(|e| Error::Index(e.to_string()))?
        } else {
            Index::create(dir, schema, tantivy::IndexSettings::default())
                .map_err(|e| Error::Index(e.to_string()))?
        };

        let writer = index
            .writer(50_000_000) // 50MB heap
            .map_err(|e| Error::Index(e.to_string()))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| Error::Index(e.to_string()))?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            id_field,
            content_field,
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let (schema, id_field, content_field) = build_schema();

        let index = Index::create_in_ram(schema);

        let writer = index
            .writer(50_000_000)
            .map_err(|e| Error::Index(e.to_string()))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| Error::Index(e.to_string()))?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            id_field,
            content_field,
        })
    }
}

impl FullTextIndex for TantivyFullTextIndex {
    fn add(&self, id: Uuid, content: &str) -> Result<()> {
        let writer = self.writer.lock().map_err(|e| Error::Index(e.to_string()))?;

        // Remove existing doc with this ID first
        let id_term = tantivy::Term::from_field_text(self.id_field, &id.to_string());
        writer.delete_term(id_term);

        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, id.to_string());
        doc.add_text(self.content_field, content);
        writer.add_document(doc).map_err(|e| Error::Index(e.to_string()))?;
        Ok(())
    }

    fn remove(&self, id: Uuid) -> Result<()> {
        let writer = self.writer.lock().map_err(|e| Error::Index(e.to_string()))?;
        let id_term = tantivy::Term::from_field_text(self.id_field, &id.to_string());
        writer.delete_term(id_term);
        Ok(())
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<(Uuid, f32)>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);
        let parsed_query = query_parser
            .parse_query(query)
            .map_err(|e| Error::Index(e.to_string()))?;

        let top_docs = searcher
            .search(&parsed_query, &TopDocs::with_limit(limit))
            .map_err(|e| Error::Index(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| Error::Index(e.to_string()))?;
            if let Some(id_value) = doc.get_first(self.id_field)
                && let Some(id_str) = id_value.as_str()
                && let Ok(uuid) = Uuid::parse_str(id_str)
            {
                results.push((uuid, score));
            }
        }
        Ok(results)
    }

    fn commit(&self) -> Result<()> {
        let mut writer = self.writer.lock().map_err(|e| Error::Index(e.to_string()))?;
        writer.commit().map_err(|e| Error::Index(e.to_string()))?;
        self.reader.reload().map_err(|e| Error::Index(e.to_string()))?;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        self.commit()
    }

    fn len(&self) -> usize {
        let searcher = self.reader.searcher();
        searcher.num_docs() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tantivy_add_and_search() {
        let index = TantivyFullTextIndex::open_in_memory().unwrap();

        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let id3 = Uuid::now_v7();

        index.add(id1, "The user prefers dark mode for all applications").unwrap();
        index.add(id2, "Rust programming language is fast and safe").unwrap();
        index.add(id3, "Python is great for data science").unwrap();
        index.commit().unwrap();

        assert_eq!(index.len(), 3);

        let results = index.search("dark mode", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, id1);

        let results = index.search("Rust programming", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, id2);
    }

    #[test]
    fn test_tantivy_remove() {
        let index = TantivyFullTextIndex::open_in_memory().unwrap();

        let id1 = Uuid::now_v7();
        index.add(id1, "test content to remove").unwrap();
        index.commit().unwrap();
        assert_eq!(index.len(), 1);

        index.remove(id1).unwrap();
        index.commit().unwrap();
        assert_eq!(index.len(), 0);

        let results = index.search("test content", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_tantivy_save_and_load() {
        let dir = std::env::temp_dir().join(format!("tantivy_test_{}", Uuid::now_v7()));

        let id1 = Uuid::now_v7();
        {
            let index = TantivyFullTextIndex::new(&dir).unwrap();
            index.add(id1, "persistent test content").unwrap();
            index.commit().unwrap();
            index.save().unwrap();
        }

        // Reopen
        {
            let index = TantivyFullTextIndex::new(&dir).unwrap();
            assert_eq!(index.len(), 1);
            let results = index.search("persistent", 10).unwrap();
            assert!(!results.is_empty());
            assert_eq!(results[0].0, id1);
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
