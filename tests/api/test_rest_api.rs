// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

// tests/api/test_rest_api.rs
use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde_json::json;
use std::time::Duration;
use tokio;

const API_BASE_URL: &str = "http://localhost:7533/api/v1";

struct TestClient {
    client: Client,
}

impl TestClient {
    fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }

    async fn health_check(&self) -> Result<serde_json::Value> {
        let response = self.client
            .get(format!("{}/health", API_BASE_URL))
            .send()
            .await?;
        
        assert_eq!(response.status(), StatusCode::OK);
        Ok(response.json().await?)
    }

    async fn insert_vector(&self, id: &str, vector: Vec<f32>, metadata: serde_json::Value) -> Result<serde_json::Value> {
        let body = json!({
            "id": id,
            "vector": vector,
            "metadata": metadata
        });

        let response = self.client
            .post(format!("{}/vectors", API_BASE_URL))
            .json(&body)
            .send()
            .await?;
        
        assert_eq!(response.status(), StatusCode::CREATED);
        Ok(response.json().await?)
    }

    async fn get_vector(&self, id: &str) -> Result<serde_json::Value> {
        let response = self.client
            .get(format!("{}/vectors/{}", API_BASE_URL, id))
            .send()
            .await?;
        
        assert_eq!(response.status(), StatusCode::OK);
        Ok(response.json().await?)
    }

    async fn search_vectors(&self, query: Vec<f32>, k: usize) -> Result<serde_json::Value> {
        let body = json!({
            "vector": query,
            "k": k,  // Note: API uses 'k' not 'limit'
            "options": {
                "include_metadata": true
            }
        });

        let response = self.client
            .post(format!("{}/search", API_BASE_URL))
            .json(&body)
            .send()
            .await?;
        
        assert_eq!(response.status(), StatusCode::OK);
        Ok(response.json().await?)
    }

    async fn batch_insert(&self, vectors: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        let body = json!({
            "vectors": vectors
        });

        let response = self.client
            .post(format!("{}/vectors/batch", API_BASE_URL))
            .json(&body)
            .send()
            .await?;
        
        assert_eq!(response.status(), StatusCode::OK);
        Ok(response.json().await?)
    }

    async fn delete_vector(&self, id: &str) -> Result<StatusCode> {
        let response = self.client
            .delete(format!("{}/vectors/{}", API_BASE_URL, id))
            .send()
            .await?;
        
        Ok(response.status())
    }
}

#[tokio::test]
async fn test_health_check() -> Result<()> {
    let client = TestClient::new();
    
    let health = client.health_check().await?;
    
    // Check structure matches API.md
    assert_eq!(health["status"], "healthy");
    assert!(health["version"].is_string());
    assert!(health["storage"]["mode"].is_string());
    assert!(health["storage"]["connected"].is_boolean());
    assert!(health["indices"]["hnsw"]["healthy"].is_boolean());
    assert!(health["indices"]["ivf"]["healthy"].is_boolean());
    
    Ok(())
}

#[tokio::test]
async fn test_vector_lifecycle() -> Result<()> {
    let client = TestClient::new();
    
    // 1. Insert a vector
    let vector_id = "test-vec-1";
    let vector_data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let metadata = json!({
        "video_id": "video_abc",
        "title": "Example Video",
        "tags": ["ai", "tutorial"]
    });
    
    let insert_result = client.insert_vector(vector_id, vector_data.clone(), metadata.clone()).await?;
    assert_eq!(insert_result["id"], vector_id);
    assert_eq!(insert_result["index"], "recent");  // API returns which index it went to
    assert!(insert_result["timestamp"].is_string());
    
    // 2. Get the vector back
    let retrieved = client.get_vector(vector_id).await?;
    assert_eq!(retrieved["id"], vector_id);
    assert_eq!(retrieved["vector"], json!(vector_data));
    assert_eq!(retrieved["metadata"], metadata);
    assert!(retrieved["timestamp"].is_string());
    
    // 3. Search should find it
    let search_results = client.search_vectors(vec![1.1, 2.1, 3.1, 4.1, 5.1], 5).await?;
    let results = search_results["results"].as_array().unwrap();
    assert!(results.len() > 0, "Search should return at least one result");
    
    // Check result structure matches API.md
    let first_result = &results[0];
    assert_eq!(first_result["id"], vector_id);
    assert!(first_result["distance"].is_number());
    assert!(first_result["score"].is_number());
    assert_eq!(first_result["metadata"], metadata);
    
    // 4. Delete the vector
    let delete_status = client.delete_vector(vector_id).await?;
    assert_eq!(delete_status, StatusCode::NO_CONTENT);
    
    // 5. Get should now return 404
    let get_response = client.client
        .get(format!("{}/vectors/{}", API_BASE_URL, vector_id))
        .send()
        .await?;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    
    Ok(())
}

#[tokio::test]
async fn test_batch_insert() -> Result<()> {
    let client = TestClient::new();
    
    // Create batch of vectors
    let vectors = vec![
        json!({
            "id": "batch-1",
            "vector": [0.1, 0.2, 0.3],
            "metadata": {"batch": true}
        }),
        json!({
            "id": "batch-2", 
            "vector": [0.4, 0.5, 0.6],
            "metadata": {"batch": true}
        }),
        json!({
            "id": "batch-3",
            "vector": [0.7, 0.8, 0.9],
            "metadata": {"batch": true}
        }),
    ];
    
    let batch_result = client.batch_insert(vectors).await?;
    
    // Check response matches API.md format
    assert_eq!(batch_result["successful"], 3);
    assert_eq!(batch_result["failed"], 0);
    assert!(batch_result["errors"].as_array().unwrap().is_empty());
    
    // Clean up
    for i in 1..=3 {
        client.delete_vector(&format!("batch-{}", i)).await?;
    }
    
    Ok(())
}

#[tokio::test]
async fn test_search_with_options() -> Result<()> {
    let client = TestClient::new();
    
    // Insert test vectors
    let vectors = vec![
        ("search-x", vec![1.0, 0.0, 0.0], json!({"label": "x-axis", "tags": ["geometry"]})),
        ("search-y", vec![0.0, 1.0, 0.0], json!({"label": "y-axis", "tags": ["geometry"]})),
        ("search-z", vec![0.0, 0.0, 1.0], json!({"label": "z-axis", "tags": ["geometry"]})),
    ];
    
    for (id, vector, metadata) in &vectors {
        client.insert_vector(id, vector.clone(), metadata.clone()).await?;
    }
    
    // Search with advanced options
    let body = json!({
        "vector": [0.9, 0.1, 0.0],
        "k": 3,
        "filter": {
            "tags": ["geometry"]
        },
        "options": {
            "search_recent": true,
            "search_historical": false,
            "hnsw_ef": 50,
            "include_metadata": true,
            "score_threshold": 0.5
        }
    });
    
    let response = client.client
        .post(format!("{}/search", API_BASE_URL))
        .json(&body)
        .send()
        .await?;
    
    assert_eq!(response.status(), StatusCode::OK);
    let search_results: serde_json::Value = response.json().await?;
    
    let results = search_results["results"].as_array().unwrap();
    assert!(results.len() >= 1);
    assert_eq!(results[0]["id"], "search-x");
    
    // Clean up
    for (id, _, _) in &vectors {
        client.delete_vector(id).await?;
    }
    
    Ok(())
}

#[tokio::test]
async fn test_error_cases() -> Result<()> {
    let client = TestClient::new();
    
    // Test getting non-existent vector
    let response = client.client
        .get(format!("{}/vectors/non-existent", API_BASE_URL))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    
    // Test invalid vector dimension (if enforced)
    let response = client.client
        .post(format!("{}/vectors", API_BASE_URL))
        .json(&json!({
            "id": "invalid-dim",
            "vector": [], // Empty vector
            "metadata": {}
        }))
        .send()
        .await?;
    
    // Should return bad request or unprocessable entity
    assert!(response.status().is_client_error());
    
    Ok(())
}