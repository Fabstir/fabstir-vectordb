use crate::api::rest::*;
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub max_retries: u32,
    pub auth_token: Option<String>,
}

#[derive(Clone)]
pub struct VectorDbClient {
    #[doc(hidden)]
    pub config: ClientConfig,
    #[doc(hidden)]
    pub client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorData {
    pub id: String,
    pub vector: Vec<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertResult {
    pub id: String,
    pub index: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub results: Vec<SearchResultItem>,
    #[serde(rename = "search_time_ms")]
    pub search_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub distance: f32,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum SearchIndex {
    Recent,
    Historical,
}

#[derive(Debug, Clone)]
pub enum VectorUpdate {
    Inserted {
        id: String,
        index: String,
    },
    Updated {
        id: String,
    },
    Deleted {
        id: String,
    },
    Migrated {
        id: String,
        from: String,
        to: String,
    },
}

pub struct UpdateStream {
    receiver: mpsc::Receiver<VectorUpdate>,
}

impl UpdateStream {
    pub async fn recv(&mut self) -> Option<VectorUpdate> {
        self.receiver.recv().await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Authentication failed")]
    AuthenticationFailed,
}

pub struct SearchBuilder {
    #[doc(hidden)]
    pub client: Arc<VectorDbClient>,
    #[doc(hidden)]
    pub vector: Vec<f32>,
    #[doc(hidden)]
    pub k: usize,
    #[doc(hidden)]
    pub filter: Option<serde_json::Value>,
    #[doc(hidden)]
    pub timeout: Option<Duration>,
    #[doc(hidden)]
    pub indices: Option<Vec<SearchIndex>>,
    #[doc(hidden)]
    pub score_threshold: Option<f32>,
}

impl SearchBuilder {
    pub fn k(mut self, k: usize) -> Self {
        self.k = k;
        self
    }

    pub fn filter(mut self, key: &str, value: &str) -> Self {
        let filter = serde_json::json!({ key: value });
        self.filter = Some(filter);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn indices(mut self, indices: Vec<SearchIndex>) -> Self {
        self.indices = Some(indices);
        self
    }

    pub fn score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }

    pub async fn execute(self) -> Result<SearchResults, ClientError> {
        let url = format!("{}/search", self.client.config.base_url);

        let mut options = SearchOptions::default();
        if let Some(timeout) = self.timeout {
            options.timeout_ms = Some(timeout.as_millis() as u64);
        }
        if let Some(indices) = self.indices {
            options.search_recent = Some(indices.iter().any(|i| matches!(i, SearchIndex::Recent)));
            options.search_historical =
                Some(indices.iter().any(|i| matches!(i, SearchIndex::Historical)));
        }
        if let Some(threshold) = self.score_threshold {
            options.score_threshold = Some(threshold);
        }

        let request = SearchRequest {
            vector: self.vector,
            k: self.k,
            filter: self.filter,
            options: Some(options),
        };

        let response = self
            .client
            .execute_with_retry(|| {
                let client = self.client.client.clone();
                let url = url.clone();
                let request = request.clone();
                async move { client.post(&url).json(&request).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: SearchResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(SearchResults {
                results: resp
                    .results
                    .into_iter()
                    .map(|r| SearchResultItem {
                        id: r.id,
                        distance: r.distance,
                        score: r.score,
                        metadata: r.metadata,
                    })
                    .collect(),
                search_time_ms: resp.search_time_ms,
            })
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }
}

pub struct BackupBuilder {
    client: Arc<VectorDbClient>,
    path: String,
    compressed: bool,
}

impl BackupBuilder {
    pub fn compressed(mut self, compressed: bool) -> Self {
        self.compressed = compressed;
        self
    }

    pub async fn execute(self) -> Result<BackupResponse, ClientError> {
        let url = format!("{}/admin/backup", self.client.config.base_url);
        let request = BackupRequest {
            backup_path: self.path,
            compress: self.compressed,
        };

        let response = self
            .client
            .execute_with_retry(|| {
                let client = self.client.client.clone();
                let url = url.clone();
                let request = request.clone();
                async move { client.post(&url).json(&request).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: BackupResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(resp)
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }
}

impl VectorDbClient {
    pub fn new(config: ClientConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    pub async fn is_healthy(&self) -> Result<bool, ClientError> {
        let url = format!("{}/health", self.config.base_url);

        match self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.get(&url).send().await }
            })
            .await
        {
            Ok(response) => Ok(response.status() == StatusCode::OK),
            Err(e) => Err(e),
        }
    }

    pub async fn insert_vector(&self, vector: VectorData) -> Result<InsertResult, ClientError> {
        if vector.id.is_empty() {
            return Err(ClientError::ValidationError(
                "Vector ID cannot be empty".to_string(),
            ));
        }

        let url = format!("{}/vectors", self.config.base_url);
        let request = InsertVectorRequest {
            id: vector.id.clone(),
            vector: vector.vector,
            metadata: vector
                .metadata
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        };

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                let request = request.clone();
                async move { client.post(&url).json(&request).send().await }
            })
            .await?;

        if response.status() == StatusCode::CREATED {
            let resp: InsertVectorResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(InsertResult {
                id: resp.id,
                index: resp.index,
                timestamp: Some(resp.timestamp),
            })
        } else if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(ClientError::ValidationError(error_text))
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub async fn get_vector(&self, id: &str) -> Result<VectorData, ClientError> {
        let url = format!("{}/vectors/{}", self.config.base_url, id);

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.get(&url).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let data: serde_json::Value = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;

            Ok(VectorData {
                id: data["id"].as_str().unwrap_or("").to_string(),
                vector: data["vector"]
                    .as_array()
                    .ok_or_else(|| ClientError::ServerError("Missing vector field".to_string()))?
                    .iter()
                    .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                    .collect(),
                metadata: data.get("metadata").cloned(),
            })
        } else if response.status() == StatusCode::NOT_FOUND {
            Err(ClientError::NotFound(format!("Vector {} not found", id)))
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub async fn update_vector(&self, vector: VectorData) -> Result<(), ClientError> {
        if vector.id.is_empty() {
            return Err(ClientError::ValidationError(
                "Vector ID cannot be empty".to_string(),
            ));
        }

        // Update is implemented as insert with the same ID
        self.insert_vector(vector).await?;
        Ok(())
    }

    pub async fn delete_vector(&self, id: &str) -> Result<(), ClientError> {
        let url = format!("{}/vectors/{}", self.config.base_url, id);

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.delete(&url).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            Ok(())
        } else if response.status() == StatusCode::NOT_FOUND {
            Err(ClientError::NotFound(format!("Vector {} not found", id)))
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub async fn batch_insert(&self, vectors: Vec<VectorData>) -> Result<BatchResult, ClientError> {
        let url = format!("{}/vectors/batch", self.config.base_url);
        let request = BatchInsertRequest {
            vectors: vectors
                .into_iter()
                .map(|v| InsertVectorRequest {
                    id: v.id,
                    vector: v.vector,
                    metadata: v
                        .metadata
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                })
                .collect(),
        };

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                let request = request.clone();
                async move { client.post(&url).json(&request).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: BatchInsertResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(BatchResult {
                successful: resp.successful,
                failed: resp.failed,
                errors: resp.errors.into_iter().map(|e| e.error).collect(),
            })
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub fn search(&self, vector: Vec<f32>) -> SearchBuilder {
        SearchBuilder {
            client: Arc::new(self.clone()),
            vector,
            k: 10,
            filter: None,
            timeout: None,
            indices: None,
            score_threshold: None,
        }
    }

    pub async fn subscribe_updates(&self) -> Result<UpdateStream, ClientError> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/stream/updates", self.config.base_url);

        // Start a background task to handle SSE
        let client = self.client.clone();
        tokio::spawn(async move {
            // TODO: Implement actual SSE handling
            // For now, just simulate by sending a dummy update
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = tx
                .send(VectorUpdate::Inserted {
                    id: "stream_test".to_string(),
                    index: "recent".to_string(),
                })
                .await;
        });

        Ok(UpdateStream { receiver: rx })
    }

    pub async fn get_statistics(&self) -> Result<StatisticsResponse, ClientError> {
        let url = format!("{}/admin/statistics", self.config.base_url);

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.get(&url).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: StatisticsResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(resp)
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub async fn trigger_migration(&self) -> Result<MigrationResponse, ClientError> {
        let url = format!("{}/admin/migrate", self.config.base_url);

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.post(&url).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: MigrationResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(resp)
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub async fn trigger_rebalance(&self) -> Result<RebalanceResponse, ClientError> {
        let url = format!("{}/admin/rebalance", self.config.base_url);

        let response = self
            .execute_with_retry(|| {
                let client = self.client.clone();
                let url = url.clone();
                async move { client.post(&url).send().await }
            })
            .await?;

        if response.status() == StatusCode::OK {
            let resp: RebalanceResponse = response.json().await.map_err(|e| {
                ClientError::ServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(resp)
        } else {
            Err(ClientError::ServerError(format!(
                "Unexpected status: {}",
                response.status()
            )))
        }
    }

    pub fn create_backup(&self, path: &str) -> BackupBuilder {
        BackupBuilder {
            client: Arc::new(self.clone()),
            path: path.to_string(),
            compressed: false,
        }
    }

    async fn execute_with_retry<F, Fut>(&self, f: F) -> Result<Response, ClientError>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<Response, reqwest::Error>> + Send,
    {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts <= self.config.max_retries {
            match f().await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);
                    attempts += 1;

                    if attempts <= self.config.max_retries {
                        tokio::time::sleep(Duration::from_millis(100 * attempts as u64)).await;
                    }
                }
            }
        }

        Err(ClientError::NetworkError(
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string()),
        ))
    }
}
