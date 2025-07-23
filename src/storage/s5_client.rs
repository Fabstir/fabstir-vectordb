use anyhow::{Result, anyhow};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::types::S5Metadata;
use crate::storage::s5_storage::S5Config;

#[derive(Debug, Clone)]
pub struct S5Client {
    base_url: String,
    api_key: Option<String>,
    http_client: Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub cid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathResponse {
    pub cid: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String, // "file" or "directory"
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    pub entries: Vec<DirectoryEntry>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchResult {
    pub path: String,
    pub cid: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

impl S5Client {
    pub fn new(config: S5Config) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            base_url: config.node_url,
            api_key: config.api_key,
            http_client,
        }
    }
    
    pub async fn health_check(&self) -> Result<()> {
        // Check if S5 node is accessible
        let response = self.http_client
            .get(&format!("{}/health", self.base_url))
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow!("S5 node health check failed"))
        }
    }
    
    pub async fn upload_data(&self, data: Vec<u8>) -> Result<String> {
        // Upload raw data to S5
        let mut request = self.http_client
            .post(&format!("{}/s5/upload", self.base_url))
            .body(data);
        
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request.send().await?;
        
        if response.status().is_success() {
            let upload_resp: UploadResponse = response.json().await?;
            Ok(upload_resp.cid)
        } else {
            Err(anyhow!("Upload failed: {}", response.status()))
        }
    }
    
    pub async fn download_data(&self, cid: &str) -> Result<Vec<u8>> {
        // Download data by CID
        let cid_part = cid.strip_prefix("s5://").unwrap_or(cid);
        
        let response = self.http_client
            .get(&format!("{}/s5/download/{}", self.base_url, cid_part))
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(response.bytes().await?.to_vec())
        } else if response.status() == StatusCode::NOT_FOUND {
            Err(anyhow!("CID not found: {}", cid))
        } else {
            Err(anyhow!("Download failed: {}", response.status()))
        }
    }
    
    // Path-based API methods (matching enhanced s5.js)
    pub async fn put_path(&self, path: &str, data: Vec<u8>) -> Result<PathResponse> {
        let mut request = self.http_client
            .put(&format!("{}/s5/fs/{}", self.base_url, path))
            .header("Content-Type", "application/cbor")
            .body(data);
        
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request.send().await?;
        
        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Path PUT failed: {}", response.status()))
        }
    }
    
    pub async fn get_path(&self, path: &str) -> Result<Vec<u8>> {
        let response = self.http_client
            .get(&format!("{}/s5/fs/{}", self.base_url, path))
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(response.bytes().await?.to_vec())
        } else {
            Err(anyhow!("Path GET failed: {}", response.status()))
        }
    }
    
    pub async fn list_path(&self, path: &str) -> Result<Vec<DirectoryEntry>> {
        // Add trailing slash for directory listing
        let url = if path.is_empty() {
            format!("{}/s5/fs/", self.base_url)
        } else {
            format!("{}/s5/fs/{}/", self.base_url, path)
        };
        
        let response = self.http_client
            .get(&url)
            .send()
            .await?;
        
        if response.status().is_success() {
            let list_resp: ListResponse = response.json().await?;
            Ok(list_resp.entries)
        } else {
            Err(anyhow!("List failed: {}", response.status()))
        }
    }
    
    pub async fn delete_path(&self, path: &str) -> Result<()> {
        let mut request = self.http_client
            .delete(&format!("{}/s5/fs/{}", self.base_url, path));
        
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request.send().await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow!("Delete failed: {}", response.status()))
        }
    }
    
    pub async fn get_metadata(&self, cid: &str) -> Result<S5Metadata> {
        let cid_part = cid.strip_prefix("s5://").unwrap_or(cid);
        
        let response = self.http_client
            .get(&format!("{}/s5/metadata/{}", self.base_url, cid_part))
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Get metadata failed: {}", response.status()))
        }
    }
    
    // Batch operations
    pub async fn batch_upload(&self, items: Vec<(&str, Vec<u8>)>) -> Result<Vec<BatchResult>> {
        // For now, sequential uploads since mock API doesn't have batch endpoint
        let mut results = Vec::new();
        
        for (path, data) in items {
            match self.put_path(path, data).await {
                Ok(resp) => results.push(BatchResult {
                    path: path.to_string(),
                    cid: Some(resp.cid),
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(BatchResult {
                    path: path.to_string(),
                    cid: None,
                    success: false,
                    error: Some(e.to_string()),
                }),
            }
        }
        
        Ok(results)
    }
}

// Retry logic implementation
impl S5Client {
    pub async fn upload_data_with_retry(&self, data: Vec<u8>) -> Result<String> {
        let mut attempts = 0;
        let max_attempts = 3;
        
        loop {
            match self.upload_data(data.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) if attempts < max_attempts - 1 => {
                    attempts += 1;
                    let delay = Duration::from_millis(100 * 2u64.pow(attempts));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}