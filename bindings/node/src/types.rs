// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use napi_derive::napi;
use serde_json;

#[napi(object)]
pub struct VectorDBConfig {
    /// S5 portal URL (e.g., "http://localhost:5524")
    pub s5_portal: String,

    /// User's blockchain-derived seed phrase
    pub user_seed_phrase: String,

    /// Unique session identifier
    pub session_id: String,

    /// Optional: Memory budget in MB (default: 512)
    pub memory_budget_mb: Option<u32>,

    /// Optional: Enable debug logging (default: false)
    pub debug: Option<bool>,

    /// Optional: Encrypt vectors at rest in S5 storage (default: true)
    pub encrypt_at_rest: Option<bool>,

    /// Optional: Vectors per chunk for storage (default: 10000)
    pub chunk_size: Option<u32>,

    /// Optional: Cache size in MB for chunk loading (default: 150)
    pub cache_size_mb: Option<u32>,
}

#[napi(object)]
pub struct LoadOptions {
    /// Load HNSW immediately, IVF on-demand (default: true)
    pub lazy_load: Option<bool>,

    /// Override session memory budget
    pub memory_budget_mb: Option<u32>,
}

#[napi(object)]
pub struct SearchOptions {
    /// Minimum similarity score (0-1, default: 0.7)
    pub threshold: Option<f64>,

    /// Include vectors in results (default: false)
    pub include_vectors: Option<bool>,

    /// Metadata filter (MongoDB-style query, optional)
    /// Examples:
    /// - `{ "category": "technology" }` - Equals filter
    /// - `{ "status": { "$in": ["active", "pending"] } }` - In filter
    /// - `{ "views": { "$gte": 1000, "$lte": 5000 } }` - Range filter
    /// - `{ "$and": [{ "category": "tech" }, { "published": true }] }` - And combinator
    pub filter: Option<serde_json::Value>,
}

#[napi(object)]
pub struct VectorInput {
    /// Unique identifier
    pub id: String,

    /// Dense embedding vector
    pub vector: Vec<f64>,

    /// Associated metadata (any valid JSON value)
    pub metadata: serde_json::Value,
}

#[napi(object)]
pub struct SearchResult {
    /// Vector ID
    pub id: String,

    /// Similarity score (0-1)
    pub score: f64,

    /// Associated metadata (any valid JSON value)
    pub metadata: serde_json::Value,

    /// Original vector (if requested)
    pub vector: Option<Vec<f64>>,
}

#[napi(object)]
pub struct SessionStats {
    /// Total active (non-deleted) vectors in index
    pub vector_count: u32,

    /// Current memory usage in MB
    pub memory_usage_mb: f64,

    /// Active index type
    pub index_type: String,

    /// Active vectors in HNSW index
    pub hnsw_vector_count: Option<u32>,

    /// Active vectors in IVF index
    pub ivf_vector_count: Option<u32>,

    /// Number of soft-deleted vectors in HNSW index
    pub hnsw_deleted_count: Option<u32>,

    /// Number of soft-deleted vectors in IVF index
    pub ivf_deleted_count: Option<u32>,

    /// Total number of soft-deleted vectors
    pub total_deleted_count: Option<u32>,
}

#[napi(object)]
pub struct DeleteResult {
    /// Number of vectors successfully deleted
    pub deleted_count: u32,

    /// IDs of deleted vectors
    pub deleted_ids: Vec<String>,
}

#[napi(object)]
pub struct VacuumStats {
    /// Number of vectors removed from HNSW index
    pub hnsw_removed: u32,

    /// Number of vectors removed from IVF index
    pub ivf_removed: u32,

    /// Total number of vectors removed
    pub total_removed: u32,
}
