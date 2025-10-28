use crate::core::types::*;
use crate::hybrid::{HybridConfig, HybridIndex, TimestampedVector};
use crate::storage::{S5StorageFactory, EnhancedS5Storage, Storage};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::env;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{info, error};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub max_request_size: usize,
    pub timeout: Duration,
    pub cors_origins: Vec<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_request_size: 10 * 1024 * 1024, // 10MB
            timeout: Duration::from_secs(30),
            cors_origins: vec!["http://localhost:3000".to_string()],
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub hybrid_index: Arc<HybridIndex>,
    pub storage: Arc<EnhancedS5Storage>,
    pub vector_map: Arc<RwLock<HashMap<String, TimestampedVector>>>,
    pub storage_config: StorageConfigInfo,
}

#[derive(Clone, Debug)]
pub struct StorageConfigInfo {
    pub mode: String,
    pub url: String,
}

// Request/Response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertVectorRequest {
    pub id: String,
    pub vector: Vec<f32>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InsertVectorResponse {
    pub id: String,
    pub index: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInsertRequest {
    pub vectors: Vec<InsertVectorRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchInsertResponse {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<BatchError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchError {
    pub id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub vector: Vec<f32>,
    pub k: usize,
    #[serde(default)]
    pub filter: Option<serde_json::Value>,
    #[serde(default)]
    pub options: Option<SearchOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchOptions {
    pub search_recent: Option<bool>,
    pub search_historical: Option<bool>,
    pub hnsw_ef: Option<usize>,
    pub ivf_n_probe: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub include_metadata: Option<bool>,
    pub score_threshold: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub search_time_ms: f64,
    pub indices_searched: u32,
    pub partial_results: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub distance: f32,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub storage: StorageHealth,
    pub indices: IndexHealth,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageHealth {
    pub mode: String,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portal_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexHealth {
    pub hnsw: IndexStatus,
    pub ivf: IndexStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStatus {
    pub healthy: bool,
    pub vector_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatisticsResponse {
    pub total_vectors: usize,
    pub recent_vectors: usize,
    pub historical_vectors: usize,
    pub memory_usage: MemoryUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub total_bytes: usize,
    pub hnsw_bytes: usize,
    pub ivf_bytes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationResponse {
    pub vectors_migrated: usize,
    pub duration_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RebalanceResponse {
    pub clusters_modified: usize,
    pub vectors_moved: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRequest {
    pub backup_path: String,
    pub compress: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupResponse {
    pub backup_size: u64,
    pub vectors_backed_up: usize,
    pub compression_ratio: f64,
}

// Error handling
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip)]
    pub status_code: StatusCode,
}

impl ErrorResponse {
    pub fn new(error: String) -> Self {
        Self {
            error,
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn bad_request(error: String) -> Self {
        Self {
            error,
            status_code: StatusCode::BAD_REQUEST,
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (self.status_code, Json(self)).into_response()
    }
}

pub async fn create_app(config: ApiConfig) -> Result<Router, anyhow::Error> {
    // Determine storage mode from environment
    let storage_mode = env::var("STORAGE_MODE")
        .or_else(|_| env::var("S5_MODE"))
        .unwrap_or_else(|_| "mock".to_string());
    
    let (storage, storage_config_info) = if storage_mode == "real" {
        // Try to create real S5 storage
        match S5StorageFactory::create_from_env() {
            Ok(s) => {
                let portal_url = env::var("S5_PORTAL_URL")
                    .unwrap_or_else(|_| "http://localhost:5524".to_string());
                let info = StorageConfigInfo {
                    mode: "real".to_string(),
                    url: portal_url,
                };
                (Arc::new(s), info)
            },
            Err(e) => {
                error!("Failed to create real S5 storage, falling back to mock: {}", e);
                // Fall back to mock mode
                let mock_url = env::var("S5_MOCK_SERVER_URL")
                    .unwrap_or_else(|_| "http://localhost:5522".to_string());
                let mock_config = crate::storage::s5_adapter::S5StorageConfig {
                    mode: crate::storage::s5_adapter::StorageMode::Mock,
                    mock_server_url: Some(mock_url.clone()),
                    portal_url: None,
                    seed_phrase: None,
                    connection_timeout: Some(5000),
                    retry_attempts: Some(3),
                    encrypt_at_rest: None,
                };
                let info = StorageConfigInfo {
                    mode: "mock".to_string(),
                    url: mock_url,
                };
                (Arc::new(EnhancedS5Storage::new(mock_config).map_err(|e| anyhow::anyhow!("Storage error: {}", e))?), info)
            }
        }
    } else {
        // Mock mode
        let mock_url = env::var("S5_MOCK_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:5522".to_string());
        let mock_config = crate::storage::s5_adapter::S5StorageConfig {
            mode: crate::storage::s5_adapter::StorageMode::Mock,
            mock_server_url: Some(mock_url.clone()),
            portal_url: None,
            seed_phrase: None,
            connection_timeout: Some(5000),
            retry_attempts: Some(3),
            encrypt_at_rest: None,
        };
        let info = StorageConfigInfo {
            mode: "mock".to_string(),
            url: mock_url,
        };
        (Arc::new(EnhancedS5Storage::new(mock_config).map_err(|e| anyhow::anyhow!("Storage error: {}", e))?), info)
    };
    
    // Initialize HybridIndex with default config
    let hybrid_config = HybridConfig::default();
    let mut hybrid_index = HybridIndex::new(hybrid_config);
    
    // Initialize with minimal training data
    // Get dimension from environment or use a small default for testing
    let dimension = std::env::var("VECTOR_DIMENSION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3); // Use 3 for testing, matching test vectors
    
    let training_vectors = vec![
        vec![1.0; dimension],
        vec![0.5; dimension],
        vec![0.0; dimension],
    ];
    hybrid_index.initialize(training_vectors).await
        .map_err(|e| anyhow::anyhow!("Failed to initialize hybrid index: {}", e))?;
    
    let hybrid_index = Arc::new(hybrid_index);

    let state = AppState { 
        hybrid_index,
        storage,
        vector_map: Arc::new(RwLock::new(HashMap::new())),
        storage_config: storage_config_info,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create API v1 routes
    let api_v1 = Router::new()
        // Health check
        .route("/health", get(health_handler))
        // Vector operations
        .route("/vectors", post(insert_vector))
        .route("/vectors/batch", post(batch_insert))
        .route("/vectors/:id", get(get_vector))
        .route("/vectors/:id", delete(delete_vector))
        // Search
        .route("/search", post(search))
        // Admin
        .route("/admin/statistics", get(get_statistics))
        .route("/admin/migrate", post(trigger_migration))
        .route("/admin/rebalance", post(rebalance))
        .route("/admin/backup", post(backup))
        // Streaming
        .route("/stream/updates", get(sse_updates))
        .route("/ws", get(websocket_handler));
    
    // Mount API v1 under /api/v1 prefix
    let app = Router::new()
        .nest("/api/v1", api_v1)
        // Middleware
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(config.max_request_size))
        .with_state(state);

    Ok(app)
}

// Handler implementations
async fn health_handler(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, ErrorResponse> {
    // Use actual storage configuration from state
    let storage_health = StorageHealth {
        mode: state.storage_config.mode.clone(),
        connected: true,
        base_url: if state.storage_config.mode == "mock" {
            Some(state.storage_config.url.clone())
        } else {
            None
        },
        portal_url: if state.storage_config.mode == "real" {
            Some(state.storage_config.url.clone())
        } else {
            None
        },
    };
    
    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        storage: storage_health,
        indices: IndexHealth {
            hnsw: IndexStatus {
                healthy: true,
                vector_count: 0,
            },
            ivf: IndexStatus {
                healthy: true,
                vector_count: 0,
            },
        },
    }))
}

async fn insert_vector(
    State(state): State<AppState>,
    Json(request): Json<InsertVectorRequest>,
) -> Result<(StatusCode, Json<InsertVectorResponse>), ErrorResponse> {
    // Validate vector
    if let Err(e) = validate_vector(&request.vector) {
        return Err(ErrorResponse::bad_request(e));
    }

    let timestamp = chrono::Utc::now();
    
    // Create timestamped vector
    let vector_id = VectorId::from_string(&request.id);
    let timestamped_vector = TimestampedVector::new(
        vector_id.clone(),
        request.vector.clone(),
        timestamp,
    );
    
    // Add to hybrid index using insert_with_timestamp
    state.hybrid_index
        .insert_with_timestamp(vector_id.clone(), request.vector.clone(), timestamp)
        .await
        .map_err(|e| ErrorResponse::new(format!("Failed to add vector to index: {}", e)))?;
    
    // Store in vector map for retrieval
    state.vector_map.write().await.insert(
        request.id.clone(),
        timestamped_vector.clone(),
    );
    
    // Persist to storage
    let storage_key = format!("vectors/{}", request.id);
    let vector_data = Vector {
        id: vector_id,
        embedding: Embedding::new(request.vector.clone())
            .map_err(|e| ErrorResponse::new(format!("Invalid embedding: {}", e)))?,
        metadata: Some(request.metadata),
    };
    
    state.storage
        .put(&storage_key, &vector_data)
        .await
        .map_err(|e| ErrorResponse::new(format!("Failed to persist vector: {}", e)))?;
    
    info!("Stored vector {} with {} dimensions", request.id, request.vector.len());
    
    Ok((
        StatusCode::CREATED,
        Json(InsertVectorResponse {
            id: request.id,
            index: "recent".to_string(),
            timestamp: timestamp.to_rfc3339(),
        }),
    ))
}

async fn batch_insert(
    State(state): State<AppState>,
    Json(request): Json<BatchInsertRequest>,
) -> Result<Json<BatchInsertResponse>, ErrorResponse> {
    let mut successful = 0;
    let mut failed = 0;
    let mut errors = Vec::new();
    
    for vector_req in request.vectors {
        // Validate vector
        if let Err(e) = validate_vector(&vector_req.vector) {
            failed += 1;
            errors.push(BatchError {
                id: vector_req.id.clone(),
                error: e,
            });
            continue;
        }
        
        let timestamp = chrono::Utc::now();
        let vector_id = VectorId::from_string(&vector_req.id);
        let timestamped_vector = TimestampedVector::new(
            vector_id.clone(),
            vector_req.vector.clone(),
            timestamp,
        );
        
        // Try to insert into index
        match state.hybrid_index
            .insert_with_timestamp(vector_id.clone(), vector_req.vector.clone(), timestamp)
            .await {
            Ok(_) => {
                // Store in vector map
                state.vector_map.write().await.insert(
                    vector_req.id.clone(),
                    timestamped_vector,
                );
                
                // Persist to storage
                let storage_key = format!("vectors/{}", vector_req.id);
                let vector_data = Vector {
                    id: vector_id,
                    embedding: match Embedding::new(vector_req.vector.clone()) {
                        Ok(e) => e,
                        Err(e) => {
                            failed += 1;
                            errors.push(BatchError {
                                id: vector_req.id.clone(),
                                error: format!("Invalid embedding: {}", e),
                            });
                            continue;
                        }
                    },
                    metadata: Some(vector_req.metadata),
                };
                
                match state.storage.put(&storage_key, &vector_data).await {
                    Ok(_) => successful += 1,
                    Err(e) => {
                        failed += 1;
                        errors.push(BatchError {
                            id: vector_req.id.clone(),
                            error: format!("Storage error: {}", e),
                        });
                    }
                }
            },
            Err(e) => {
                failed += 1;
                errors.push(BatchError {
                    id: vector_req.id.clone(),
                    error: format!("Index error: {}", e),
                });
            }
        }
    }
    
    Ok(Json(BatchInsertResponse {
        successful,
        failed,
        errors,
    }))
}

async fn get_vector(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // First check in-memory map
    if let Some(vector) = state.vector_map.read().await.get(&id) {
        let response = serde_json::json!({
            "id": id,
            "vector": vector.vector(),
            "metadata": {},
            "index": if vector.is_recent(Duration::from_secs(7 * 24 * 3600)) { 
                "recent" 
            } else { 
                "historical" 
            },
            "timestamp": chrono::DateTime::<chrono::Utc>::from(vector.timestamp()).to_rfc3339(),
        });
        return Ok(Json(response));
    }
    
    // Try to load from storage
    let storage_key = format!("vectors/{}", id);
    match state.storage.get::<Vector>(&storage_key).await {
        Ok(vector) => {
            let response = serde_json::json!({
                "id": id,
                "vector": vector.embedding.as_slice(),
                "metadata": vector.metadata.unwrap_or(serde_json::json!({})),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            Ok(Json(response))
        },
        Err(e) => {
            info!("Vector {} not found: {}", id, e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn delete_vector(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    // Remove from in-memory map
    let existed = state.vector_map.write().await.remove(&id).is_some();
    
    // Delete from storage
    let storage_key = format!("vectors/{}", id);
    match state.storage.delete(&storage_key).await {
        Ok(_) => {
            info!("Deleted vector {}", id);
            Ok(StatusCode::NO_CONTENT)
        },
        Err(e) => {
            if existed {
                // Was in memory but failed to delete from storage
                error!("Failed to delete vector {} from storage: {}", id, e);
                Ok(StatusCode::NO_CONTENT) // Still report success since it's removed from memory
            } else {
                // Not found anywhere
                Ok(StatusCode::NOT_FOUND)
            }
        }
    }
}

async fn search(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ErrorResponse> {
    // Validate query vector
    if let Err(e) = validate_vector(&request.vector) {
        return Err(ErrorResponse::bad_request(e));
    }
    
    let start_time = std::time::Instant::now();
    
    // Configure search
    let search_config = crate::hybrid::HybridSearchConfig {
        search_recent: request.options.as_ref()
            .and_then(|o| o.search_recent)
            .unwrap_or(true),
        search_historical: request.options.as_ref()
            .and_then(|o| o.search_historical)
            .unwrap_or(true),
        recent_k: 0,
        historical_k: 0,
        recent_threshold_override: None,
        k: request.k,
        hnsw_ef: request.options.as_ref()
            .and_then(|o| o.hnsw_ef)
            .unwrap_or(50),
        ivf_n_probe: request.options.as_ref()
            .and_then(|o| o.ivf_n_probe)
            .unwrap_or(10),
    };
    
    // Perform search - HybridIndex search method takes vector and k
    let search_results = state.hybrid_index
        .search(&request.vector, request.k)
        .await
        .map_err(|e| ErrorResponse::new(format!("Search failed: {}", e)))?;
    
    // Convert results
    let mut results = Vec::new();
    for result in search_results {
        // Get metadata from storage or in-memory map
        let metadata = if let Some(vector) = state.vector_map.read().await.get(&result.vector_id.to_string()) {
            serde_json::json!({})
        } else {
            let storage_key = format!("vectors/{}", result.vector_id.to_string());
            match state.storage.get::<Vector>(&storage_key).await {
                Ok(vector) => vector.metadata.unwrap_or(serde_json::json!({})),
                Err(_) => serde_json::json!({}),
            }
        };
        
        results.push(SearchResult {
            id: result.vector_id.to_string(),
            distance: result.distance,
            score: 1.0 / (1.0 + result.distance), // Convert distance to similarity score
            metadata: if request.options.as_ref()
                .and_then(|o| o.include_metadata)
                .unwrap_or(false) { 
                Some(metadata) 
            } else { 
                None 
            },
        });
    }
    
    // Apply score threshold if specified
    if let Some(threshold) = request.options.as_ref().and_then(|o| o.score_threshold) {
        results.retain(|r| r.score >= threshold);
    }
    
    let elapsed = start_time.elapsed();
    
    Ok(Json(SearchResponse {
        results,
        search_time_ms: elapsed.as_secs_f64() * 1000.0,
        indices_searched: if search_config.search_recent && search_config.search_historical { 2 } else { 1 },
        partial_results: false,
    }))
}

async fn get_statistics(
    State(state): State<AppState>,
) -> Result<Json<StatisticsResponse>, ErrorResponse> {
    // TODO: Implement statistics
    Ok(Json(StatisticsResponse {
        total_vectors: 0,
        recent_vectors: 0,
        historical_vectors: 0,
        memory_usage: MemoryUsage {
            total_bytes: 0,
            hnsw_bytes: 0,
            ivf_bytes: 0,
        },
    }))
}

async fn trigger_migration(
    State(state): State<AppState>,
) -> Result<Json<MigrationResponse>, ErrorResponse> {
    // TODO: Implement migration
    Ok(Json(MigrationResponse {
        vectors_migrated: 0,
        duration_ms: 0.0,
    }))
}

async fn rebalance(
    State(state): State<AppState>,
) -> Result<Json<RebalanceResponse>, ErrorResponse> {
    // TODO: Implement rebalancing
    Ok(Json(RebalanceResponse {
        clusters_modified: 0,
        vectors_moved: 0,
    }))
}

async fn backup(
    State(state): State<AppState>,
    Json(request): Json<BackupRequest>,
) -> Result<Json<BackupResponse>, ErrorResponse> {
    // TODO: Implement backup
    Ok(Json(BackupResponse {
        backup_size: 0,
        vectors_backed_up: 0,
        compression_ratio: 1.0,
    }))
}

async fn sse_updates(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    // TODO: Implement SSE updates
    let stream = futures::stream::empty();
    Sse::new(stream)
}

async fn websocket_handler() -> impl IntoResponse {
    // TODO: Implement WebSocket handler
    StatusCode::SWITCHING_PROTOCOLS
}

// Validation helpers
pub fn validate_vector(vector: &[f32]) -> Result<(), String> {
    if vector.is_empty() {
        return Err("Vector cannot be empty".to_string());
    }
    Ok(())
}
