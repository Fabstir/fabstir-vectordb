use crate::core::types::*;
use crate::hybrid::{HybridConfig, HybridIndex};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

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
    // Initialize HybridIndex with default config
    let hybrid_config = HybridConfig::default();
    let hybrid_index = Arc::new(HybridIndex::new(hybrid_config));

    let state = AppState { hybrid_index };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
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
        .route("/ws", get(websocket_handler))
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
    // TODO: Get actual storage info from HybridIndex once it exposes storage
    // For now, return default mock storage info
    let storage_health = StorageHealth {
        mode: "mock".to_string(),
        connected: true,
        base_url: Some("http://localhost:5524".to_string()),
        portal_url: None,
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

    // TODO: Implement actual vector insertion to hybrid index
    Ok((
        StatusCode::CREATED,
        Json(InsertVectorResponse {
            id: request.id,
            index: "recent".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }),
    ))
}

async fn batch_insert(
    State(state): State<AppState>,
    Json(request): Json<BatchInsertRequest>,
) -> Result<Json<BatchInsertResponse>, ErrorResponse> {
    // TODO: Implement batch insertion
    Ok(Json(BatchInsertResponse {
        successful: request.vectors.len(),
        failed: 0,
        errors: vec![],
    }))
}

async fn get_vector(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Implement vector retrieval
    Err(StatusCode::NOT_FOUND)
}

async fn delete_vector(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    // TODO: Implement vector deletion
    Ok(StatusCode::OK)
}

async fn search(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ErrorResponse> {
    // TODO: Implement search
    Ok(Json(SearchResponse {
        results: vec![],
        search_time_ms: 0.0,
        indices_searched: 0,
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
