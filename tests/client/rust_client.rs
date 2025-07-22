use std::time::Duration;
use tokio;
use vector_db::client::rust::*;
use vector_db::core::types::*;

#[cfg(test)]
mod rust_client_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_client_initialization() {
        let config = ClientConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            auth_token: None,
        };

        let client = VectorDbClient::new(config);
        assert!(client.is_healthy().await.is_ok());
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_vector_operations() {
        let client = create_test_client();

        // Insert vector
        let vector = VectorData {
            id: "test_vec".to_string(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: Some(serde_json::json!({
                "category": "test",
                "score": 0.95
            })),
        };

        let result = client.insert_vector(vector.clone()).await.unwrap();
        assert_eq!(result.id, "test_vec");
        assert!(result.timestamp.is_some());

        // Get vector
        let retrieved = client.get_vector("test_vec").await.unwrap();
        assert_eq!(retrieved.id, vector.id);
        assert_eq!(retrieved.vector, vector.vector);

        // Update vector
        let updated_vector = VectorData {
            id: "test_vec".to_string(),
            vector: vec![2.0, 3.0, 4.0],
            metadata: Some(serde_json::json!({
                "category": "updated"
            })),
        };

        client.update_vector(updated_vector).await.unwrap();

        // Delete vector
        client.delete_vector("test_vec").await.unwrap();

        // Verify deletion
        let result = client.get_vector("test_vec").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_batch_operations() {
        let client = create_test_client();

        let vectors = vec![
            VectorData {
                id: "batch_1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                metadata: None,
            },
            VectorData {
                id: "batch_2".to_string(),
                vector: vec![0.0, 1.0, 0.0],
                metadata: None,
            },
            VectorData {
                id: "batch_3".to_string(),
                vector: vec![0.0, 0.0, 1.0],
                metadata: None,
            },
        ];

        let result = client.batch_insert(vectors).await.unwrap();
        assert_eq!(result.successful, 3);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_search() {
        let client = create_test_client();

        // Insert test data
        setup_test_data(&client).await;

        // Basic search
        let results = client
            .search(vec![2.5, 2.5, 0.0])
            .k(5)
            .execute()
            .await
            .unwrap();

        assert!(!results.results.is_empty());
        assert!(results.results.len() <= 5);

        // Search with filters
        let filtered_results = client
            .search(vec![0.0, 0.0, 0.0])
            .k(10)
            .filter("category", "A")
            .execute()
            .await
            .unwrap();

        for result in &filtered_results.results {
            assert_eq!(result.metadata.as_ref().unwrap()["category"], "A");
        }

        // Search with options
        let advanced_results = client
            .search(vec![1.0, 1.0, 1.0])
            .k(20)
            .timeout(Duration::from_secs(5))
            .indices(vec![SearchIndex::Recent, SearchIndex::Historical])
            .score_threshold(0.7)
            .execute()
            .await
            .unwrap();

        for result in &advanced_results.results {
            assert!(result.score >= 0.7);
        }
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_streaming() {
        let client = create_test_client();

        // Subscribe to updates
        let mut stream = client.subscribe_updates().await.unwrap();

        // Insert vector in parallel
        let client_clone = client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let vector = VectorData {
                id: "stream_test".to_string(),
                vector: vec![1.0, 2.0, 3.0],
                metadata: None,
            };

            client_clone.insert_vector(vector).await.unwrap();
        });

        // Receive update
        let update = tokio::time::timeout(Duration::from_secs(2), stream.recv())
            .await
            .unwrap()
            .unwrap();

        match update {
            VectorUpdate::Inserted { id, .. } => {
                assert_eq!(id, "stream_test");
            }
            _ => panic!("Expected Inserted event"),
        }
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_admin_operations() {
        let client = create_test_client();

        // Get statistics
        let stats = client.get_statistics().await.unwrap();
        assert!(stats.total_vectors >= 0);
        assert!(stats.memory_usage.total_bytes > 0);

        // Trigger migration
        let migration_result = client.trigger_migration().await.unwrap();
        assert!(migration_result.duration_ms >= 0.0);

        // Trigger rebalance
        let rebalance_result = client.trigger_rebalance().await.unwrap();
        assert!(rebalance_result.clusters_modified >= 0);

        // Create backup
        let backup_result = client
            .create_backup("/backups/test")
            .compressed(true)
            .execute()
            .await
            .unwrap();

        assert!(backup_result.backup_size > 0);
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_error_handling() {
        let client = create_test_client();

        // Test not found error
        let result = client.get_vector("nonexistent").await;
        match result {
            Err(ClientError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }

        // Test validation error
        let invalid_vector = VectorData {
            id: "".to_string(), // Invalid empty ID
            vector: vec![1.0],
            metadata: None,
        };

        let result = client.insert_vector(invalid_vector).await;
        match result {
            Err(ClientError::ValidationError(_)) => {}
            _ => panic!("Expected ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_retry_logic() {
        // Create client with flaky server
        let config = ClientConfig {
            base_url: "http://localhost:9999".to_string(), // Non-existent
            timeout: Duration::from_millis(100),
            max_retries: 2,
            auth_token: None,
        };

        let client = VectorDbClient::new(config);

        let start = tokio::time::Instant::now();
        let result = client.is_healthy().await;
        let duration = start.elapsed();

        assert!(result.is_err());
        // Should have retried
        assert!(duration >= Duration::from_millis(200));
    }

    #[test]
    fn test_search_builder() {
        let config = ClientConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            auth_token: None,
        };

        let client = VectorDbClient::new(config);

        // Test builder pattern
        let search = client
            .search(vec![1.0, 2.0, 3.0])
            .k(10)
            .filter("category", "test")
            .timeout(Duration::from_secs(5))
            .score_threshold(0.8);

        // Just verify the builder compiles and can be constructed
        assert_eq!(search.k, 10);
        assert!(search.filter.is_some());
        assert!(search.timeout.is_some());
        assert_eq!(search.score_threshold, Some(0.8));
    }

    #[test]
    fn test_client_config() {
        let config = ClientConfig {
            base_url: "https://api.example.com".to_string(),
            timeout: Duration::from_secs(60),
            max_retries: 5,
            auth_token: Some("secret-token".to_string()),
        };

        let client = VectorDbClient::new(config.clone());

        // Verify config is stored
        assert_eq!(client.config.base_url, "https://api.example.com");
        assert_eq!(client.config.timeout, Duration::from_secs(60));
        assert_eq!(client.config.max_retries, 5);
        assert_eq!(client.config.auth_token, Some("secret-token".to_string()));
    }
}

// Helper functions
fn create_test_client() -> VectorDbClient {
    VectorDbClient::new(ClientConfig {
        base_url: test_server_url(),
        timeout: Duration::from_secs(10),
        max_retries: 3,
        auth_token: None,
    })
}

fn test_server_url() -> String {
    std::env::var("TEST_SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

async fn setup_test_data(client: &VectorDbClient) {
    let vectors: Vec<VectorData> = (0..10)
        .map(|i| VectorData {
            id: format!("test_{}", i),
            vector: vec![i as f32 * 0.1, (i as f32 * 0.2).sin(), 0.0],
            metadata: Some(serde_json::json!({
                "category": if i % 3 == 0 { "A" } else { "B" },
                "index": i,
            })),
        })
        .collect();

    client.batch_insert(vectors).await.unwrap();
}
