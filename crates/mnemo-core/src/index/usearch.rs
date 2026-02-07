use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

use crate::error::{Error, Result};
use crate::index::VectorIndex;
use uuid::Uuid;

pub struct UsearchIndex {
    index: RwLock<usearch::Index>,
    uuid_to_key: RwLock<HashMap<Uuid, u64>>,
    key_to_uuid: RwLock<HashMap<u64, Uuid>>,
    next_key: RwLock<u64>,
    dimensions: usize,
}

impl UsearchIndex {
    pub fn new(dimensions: usize) -> Result<Self> {
        let opts = usearch::IndexOptions {
            dimensions,
            metric: usearch::MetricKind::Cos,
            quantization: usearch::ScalarKind::F32,
            ..Default::default()
        };
        let index = usearch::Index::new(&opts)
            .map_err(|e| Error::Index(e.to_string()))?;
        index
            .reserve(10_000)
            .map_err(|e| Error::Index(e.to_string()))?;

        Ok(Self {
            index: RwLock::new(index),
            uuid_to_key: RwLock::new(HashMap::new()),
            key_to_uuid: RwLock::new(HashMap::new()),
            next_key: RwLock::new(0),
            dimensions,
        })
    }

    fn allocate_key(&self, id: Uuid) -> u64 {
        let mut next = self.next_key.write().unwrap();
        let key = *next;
        *next += 1;
        self.uuid_to_key.write().unwrap().insert(id, key);
        self.key_to_uuid.write().unwrap().insert(key, id);
        key
    }
}

impl VectorIndex for UsearchIndex {
    fn add(&self, id: Uuid, vector: &[f32]) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(Error::Validation(format!(
                "expected {} dimensions, got {}",
                self.dimensions,
                vector.len()
            )));
        }

        // If this UUID already exists, remove it first
        if self.uuid_to_key.read().unwrap().contains_key(&id) {
            self.remove(id)?;
        }

        let key = self.allocate_key(id);
        let index = self.index.read().unwrap();

        // Grow capacity if needed
        if index.size() >= index.capacity() {
            index
                .reserve(index.capacity() + 10_000)
                .map_err(|e| Error::Index(e.to_string()))?;
        }

        index
            .add(key, vector)
            .map_err(|e| Error::Index(e.to_string()))?;
        Ok(())
    }

    fn remove(&self, id: Uuid) -> Result<()> {
        let key = {
            let map = self.uuid_to_key.read().unwrap();
            match map.get(&id) {
                Some(&k) => k,
                None => return Ok(()),
            }
        };

        let index = self.index.read().unwrap();
        index
            .remove(key)
            .map_err(|e| Error::Index(e.to_string()))?;

        self.uuid_to_key.write().unwrap().remove(&id);
        self.key_to_uuid.write().unwrap().remove(&key);
        Ok(())
    }

    fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(Uuid, f32)>> {
        let index = self.index.read().unwrap();
        let results = index
            .search(query, limit)
            .map_err(|e| Error::Index(e.to_string()))?;

        let key_map = self.key_to_uuid.read().unwrap();
        let mut output = Vec::new();
        for (key, distance) in results.keys.iter().zip(results.distances.iter()) {
            if let Some(&uuid) = key_map.get(key) {
                output.push((uuid, *distance));
            }
        }
        Ok(output)
    }

    fn filtered_search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &dyn Fn(Uuid) -> bool,
    ) -> Result<Vec<(Uuid, f32)>> {
        let index_size = self.len();
        if index_size == 0 {
            return Ok(Vec::new());
        }
        // Iterative oversample: start at 3x, double until we have enough or hit index size
        let mut oversample = (limit * 3).max(1);
        loop {
            let results = self.search(query, oversample.min(index_size))?;
            let filtered: Vec<(Uuid, f32)> = results
                .into_iter()
                .filter(|(uuid, _)| filter(*uuid))
                .take(limit)
                .collect();
            if filtered.len() >= limit || oversample >= index_size {
                return Ok(filtered);
            }
            oversample = (oversample * 2).min(index_size);
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        let index = self.index.read().unwrap();
        index
            .save(path.to_str().unwrap())
            .map_err(|e| Error::Index(e.to_string()))?;

        // Save mappings alongside
        let mappings_path = path.with_extension("mappings.json");
        let uuid_to_key = self.uuid_to_key.read().unwrap();
        let next_key = *self.next_key.read().unwrap();
        let data = serde_json::json!({
            "uuid_to_key": uuid_to_key.iter().map(|(k, v)| (k.to_string(), v)).collect::<HashMap<String, &u64>>(),
            "next_key": next_key,
        });
        std::fs::write(&mappings_path, serde_json::to_string(&data).unwrap())
            .map_err(|e| Error::Index(e.to_string()))?;
        Ok(())
    }

    fn load(&self, path: &Path) -> Result<()> {
        let index = self.index.read().unwrap();
        index
            .load(path.to_str().unwrap())
            .map_err(|e| Error::Index(e.to_string()))?;

        // Load mappings
        let mappings_path = path.with_extension("mappings.json");
        if mappings_path.exists() {
            let data = std::fs::read_to_string(&mappings_path)
                .map_err(|e| Error::Index(e.to_string()))?;
            let parsed: serde_json::Value =
                serde_json::from_str(&data).map_err(|e| Error::Index(e.to_string()))?;

            let mut uuid_to_key = self.uuid_to_key.write().unwrap();
            let mut key_to_uuid = self.key_to_uuid.write().unwrap();
            let mut next_key = self.next_key.write().unwrap();

            uuid_to_key.clear();
            key_to_uuid.clear();

            if let Some(map) = parsed["uuid_to_key"].as_object() {
                for (uuid_str, key_val) in map {
                    let uuid = Uuid::parse_str(uuid_str)
                        .map_err(|e| Error::Index(e.to_string()))?;
                    let key = key_val.as_u64().unwrap();
                    uuid_to_key.insert(uuid, key);
                    key_to_uuid.insert(key, uuid);
                }
            }

            if let Some(nk) = parsed["next_key"].as_u64() {
                *next_key = nk;
            }
        }
        Ok(())
    }

    fn len(&self) -> usize {
        let index = self.index.read().unwrap();
        index.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_vector(dims: usize, seed: u64) -> Vec<f32> {
        // Simple deterministic pseudo-random
        let mut v = Vec::with_capacity(dims);
        let mut x = seed;
        for _ in 0..dims {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            v.push((x as f32) / (u64::MAX as f32));
        }
        // Normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }

    #[test]
    fn test_add_and_search() {
        let index = UsearchIndex::new(128).unwrap();

        let mut ids = Vec::new();
        let mut vectors = Vec::new();
        for i in 0..100 {
            let id = Uuid::now_v7();
            let vec = random_vector(128, i);
            index.add(id, &vec).unwrap();
            ids.push(id);
            vectors.push(vec);
        }

        assert_eq!(index.len(), 100);

        // Search with the first vector should return itself as nearest
        let results = index.search(&vectors[0], 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, ids[0]);
    }

    #[test]
    fn test_remove() {
        let index = UsearchIndex::new(128).unwrap();
        let id = Uuid::now_v7();
        let vec = random_vector(128, 42);

        index.add(id, &vec).unwrap();
        assert_eq!(index.len(), 1);

        index.remove(id).unwrap();
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_filtered_search() {
        let index = UsearchIndex::new(128).unwrap();

        let mut ids = Vec::new();
        for i in 0..50 {
            let id = Uuid::now_v7();
            let vec = random_vector(128, i);
            index.add(id, &vec).unwrap();
            ids.push(id);
        }

        // Filter out all even-indexed IDs
        let excluded: std::collections::HashSet<Uuid> =
            ids.iter().step_by(2).copied().collect();
        let query = random_vector(128, 0);
        let results = index
            .filtered_search(&query, 10, &|id| !excluded.contains(&id))
            .unwrap();

        // All results should be odd-indexed
        for (id, _) in &results {
            assert!(!excluded.contains(id));
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = std::env::temp_dir().join(format!("usearch_test_{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let index_path = dir.join("test.usearch");

        let index = UsearchIndex::new(128).unwrap();
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        index.add(id1, &random_vector(128, 1)).unwrap();
        index.add(id2, &random_vector(128, 2)).unwrap();

        index.save(&index_path).unwrap();

        // Load into a new index
        let index2 = UsearchIndex::new(128).unwrap();
        index2.load(&index_path).unwrap();
        assert_eq!(index2.len(), 2);

        // Search should still work
        let results = index2.search(&random_vector(128, 1), 1).unwrap();
        assert_eq!(results[0].0, id1);

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_dimension_mismatch() {
        let index = UsearchIndex::new(128).unwrap();
        let result = index.add(Uuid::now_v7(), &vec![0.1; 64]);
        assert!(result.is_err());
    }
}
