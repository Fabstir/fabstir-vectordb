use napi_derive::napi;

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
}

#[napi(object)]
pub struct VectorInput {
    /// Unique identifier
    pub id: String,

    /// Dense embedding vector
    pub vector: Vec<f64>,

    /// Associated metadata (JSON string)
    pub metadata: String,
}

#[napi(object)]
pub struct SearchResult {
    /// Vector ID
    pub id: String,

    /// Similarity score (0-1)
    pub score: f64,

    /// Associated metadata (JSON string)
    pub metadata: String,

    /// Original vector (if requested)
    pub vector: Option<Vec<f64>>,
}

#[napi(object)]
pub struct SessionStats {
    /// Total vectors in index
    pub vector_count: u32,

    /// Current memory usage in MB
    pub memory_usage_mb: f64,

    /// Active index type
    pub index_type: String,

    /// Vectors in HNSW index
    pub hnsw_vector_count: Option<u32>,

    /// Vectors in IVF index
    pub ivf_vector_count: Option<u32>,
}
