use blake3;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct VectorId([u8; 32]);

impl VectorId {
    pub fn new() -> Self {
        let uuid = uuid::Uuid::new_v4();
        let hash = blake3::hash(uuid.as_bytes());
        VectorId(hash.into())
    }

    pub fn from_string(s: &str) -> Self {
        let hash = blake3::hash(s.as_bytes());
        VectorId(hash.into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn to_string(&self) -> String {
        format!("vec_{}", &self.hash_hex()[..8])
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        serde_cbor::to_vec(self)
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, serde_cbor::Error> {
        serde_cbor::from_slice(data)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    data: Vec<f32>,
}

impl Embedding {
    pub fn new(data: Vec<f32>) -> Result<Self, &'static str> {
        if data.is_empty() {
            return Err("Embedding cannot be empty");
        }
        Ok(Embedding { data })
    }

    pub fn dimension(&self) -> usize {
        self.data.len()
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    pub fn magnitude(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag == 0.0 {
            return self.clone();
        }
        let normalized: Vec<f32> = self.data.iter().map(|x| x / mag).collect();
        Embedding::new_unchecked(normalized)
    }

    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.dimension() != other.dimension() {
            panic!(
                "Dimension mismatch: {} != {}",
                self.dimension(),
                other.dimension()
            );
        }

        let dot_product: f32 = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .sum();

        let mag_a = self.magnitude();
        let mag_b = other.magnitude();

        if mag_a == 0.0 || mag_b == 0.0 {
            return 0.0;
        }

        dot_product / (mag_a * mag_b)
    }

    pub fn euclidean_distance(&self, other: &Self) -> f32 {
        if self.dimension() != other.dimension() {
            panic!(
                "Dimension mismatch: {} != {}",
                self.dimension(),
                other.dimension()
            );
        }

        self.data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt()
    }
    
    // For backward compatibility, provide a method that doesn't return Result
    pub fn new_unchecked(data: Vec<f32>) -> Self {
        Embedding { data }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vector {
    pub id: VectorId,
    pub embedding: Embedding,
    pub metadata: Option<serde_json::Value>,
}

impl Vector {
    pub fn new(id: VectorId, embedding: Embedding) -> Self {
        Self {
            id,
            embedding,
            metadata: None,
        }
    }
    
    pub fn with_metadata(id: VectorId, embedding: Embedding, metadata: serde_json::Value) -> Self {
        Self {
            id,
            embedding,
            metadata: Some(metadata),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub video_id: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub duration_seconds: u64,
    pub upload_timestamp: DateTime<Utc>,
    pub model_name: String,
    pub extra: HashMap<String, serde_json::Value>,
}

impl VideoMetadata {
    pub fn to_cbor(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        serde_cbor::to_vec(self)
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, serde_cbor::Error> {
        serde_cbor::from_slice(data)
    }
}

impl Default for VideoMetadata {
    fn default() -> Self {
        VideoMetadata {
            video_id: String::new(),
            title: String::new(),
            description: None,
            tags: Vec::new(),
            duration_seconds: 0,
            upload_timestamp: Utc::now(),
            model_name: String::new(),
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub vector_id: VectorId,
    pub distance: f32,
    pub metadata: Option<VideoMetadata>,
}

impl SearchResult {
    pub fn new(vector_id: VectorId, distance: f32, metadata: Option<VideoMetadata>) -> Self {
        SearchResult {
            vector_id,
            distance,
            metadata,
        }
    }

    pub fn deduplicate(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
        use std::collections::HashMap;

        let mut best_scores: HashMap<VectorId, SearchResult> = HashMap::new();

        for result in results.drain(..) {
            match best_scores.get(&result.vector_id) {
                Some(existing) if existing.distance <= result.distance => {}
                _ => {
                    best_scores.insert(result.vector_id.clone(), result);
                }
            }
        }

        let mut deduped: Vec<SearchResult> = best_scores.into_values().collect();
        deduped.sort();
        deduped
    }
}

impl PartialOrd for SearchResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.distance.partial_cmp(&other.distance)
    }
}

impl Ord for SearchResult {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Eq for SearchResult {}
