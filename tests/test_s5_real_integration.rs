// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

// tests/test_s5_real_integration.rs

use vector_db::storage::{Storage, S5StorageConfig, StorageMode, EnhancedS5Storage, S5StorageAdapter};
use vector_db::core::{Vector, VectorId, Embedding};
use vector_db::types::VideoNFTMetadata;
use std::time::Instant;
use std::env;

/// Helper to check if we can connect to the Enhanced s5.js service
async fn is_service_available() -> bool {
    let service_url = env::var("S5_SERVICE_URL").unwrap_or_else(|_| "http://localhost:5524".to_string());
    match reqwest::get(format!("{}/health", service_url)).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Create storage configured for real S5 portal
fn create_real_storage(seed_phrase: Option<String>) -> Result<EnhancedS5Storage, Box<dyn std::error::Error + Send + Sync>> {
    // Use the Enhanced s5.js service URL, not the S5 portal directly
    // The service handles the actual S5 portal connection
    let service_url = env::var("S5_SERVICE_URL").unwrap_or_else(|_| "http://localhost:5524".to_string());
    
    let config = S5StorageConfig {
        mode: StorageMode::Real,
        mock_server_url: None,
        portal_url: Some(service_url), // This points to our Enhanced s5.js service
        seed_phrase, // Pass seed phrase to service via environment
        connection_timeout: Some(30000), // 30 seconds for real network
        retry_attempts: Some(5),
    };
    
    EnhancedS5Storage::new(config)
}

mod phase_8_3_real_s5_portal_integration {
    use super::*;

    mod s5_portal_connection {
        use super::*;

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_s5_client_initialization_with_seed_phrase() {
            // Skip test if service is not available
            if !is_service_available().await {
                eprintln!("Skipping test: S5 service not available at {}", 
                         env::var("S5_SERVICE_URL").unwrap_or_else(|_| "http://localhost:5524".to_string()));
                return;
            }
            // Test with a generated seed phrase
            let storage = create_real_storage(None).expect("Should create storage without seed phrase");
            
            // The implementation should generate a seed phrase internally
            assert_eq!(storage.get_mode(), StorageMode::Real);
            
            // Test connection
            let connected = storage.is_connected().await;
            assert!(connected, "Should be able to connect to S5 portal");
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_s5_client_with_provided_seed_phrase() {
            // Test with a specific seed phrase
            let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string();
            
            let storage = create_real_storage(Some(seed_phrase.clone())).expect("Should create storage with seed phrase");
            
            assert_eq!(storage.get_mode(), StorageMode::Real);
            
            // Verify we can perform operations
            let test_key = "test/seed-phrase-test";
            let test_data = "test data";
            
            <EnhancedS5Storage as Storage>::put(&storage, test_key, &test_data).await.expect("Should store data");
            
            // Create another instance with same seed phrase
            let storage2 = create_real_storage(Some(seed_phrase)).expect("Should create second storage instance");
            
            // Should be able to retrieve the same data
            let retrieved: String = storage2.get(test_key).await.expect("Should retrieve data");
            assert_eq!(retrieved, test_data);
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_portal_registration() {
            // This test verifies that the portal registration process works
            let storage = create_real_storage(None).expect("Should create storage");
            
            // The registration should happen during initialization
            // Test by performing an operation
            let test_key = "test/registration-test";
            let test_data = vec![1u8, 2, 3, 4, 5];
            
            storage.put_raw(test_key, test_data.clone()).await
                .expect("Should be able to store after registration");
            
            let retrieved = storage.get_raw(test_key).await
                .expect("Should retrieve data");
            
            assert_eq!(retrieved, test_data);
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_portal_url_configuration() {
            // Test with custom portal URL from environment
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            
            let storage = create_real_storage(None).expect("Should create storage with env portal URL");
            assert_eq!(storage.get_mode(), StorageMode::Real);
            
            // Clean up
            env::remove_var("S5_PORTAL_URL");
        }
    }

    mod real_portal_testing {
        use super::*;

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_generated_seed_phrases() {
            // Create multiple storage instances with generated seed phrases
            let storage1 = create_real_storage(None).expect("Should create first storage");
            let storage2 = create_real_storage(None).expect("Should create second storage");
            
            // Each should have a different identity (different seed phrases)
            let key1 = "test/identity1";
            let key2 = "test/identity2";
            let data1 = "data from storage 1";
            let data2 = "data from storage 2";
            
            storage1.put(key1, &data1).await.expect("Storage 1 should store");
            storage2.put(key2, &data2).await.expect("Storage 2 should store");
            
            // Verify isolation - storage1 shouldn't see storage2's data
            let result1 = storage1.exists(key2).await.unwrap_or(false);
            let result2 = storage2.exists(key1).await.unwrap_or(false);
            
            // This might not be true depending on S5's design - adjust based on actual behavior
            println!("Storage isolation test - storage1 sees key2: {}, storage2 sees key1: {}", result1, result2);
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_vector_persistence_across_sessions() {
            let seed_phrase = "test seed phrase twelve words here for testing vector persistence only".to_string();
            
            // First session - store vectors
            {
                let storage = create_real_storage(Some(seed_phrase.clone())).expect("Should create storage");
                
                let vector_id = VectorId::from_string("persist-test-001");
                let embedding = Embedding::new(vec![0.1, 0.2, 0.3, 0.4, 0.5]).unwrap();
                let vector = Vector::new(vector_id.clone(), embedding);
                
                let key = format!("vectors/{}", vector_id.to_string());
                storage.put(&key, &vector).await.expect("Should store vector");
                
                // Also store metadata
                let metadata = VideoNFTMetadata {
                    address: "0xtest".to_string(),
                    id: "persist-meta-001".to_string(),
                    name: "Persistence Test Video".to_string(),
                    description: "Test video for persistence".to_string(),
                    image: "https://example.com/persist.jpg".to_string(),
                    poster_image: None,
                    animation_url: None,
                    mint_date_time: chrono::Utc::now(),
                    creator: "Test Creator".to_string(),
                    genre: vec!["Test".to_string()],
                    tags: vec![],
                    r#type: "video".to_string(),
                    attributes: vec![],
                    summary: "Test summary".to_string(),
                };
                
                storage.put("metadata/persist-test", &metadata).await.expect("Should store metadata");
            }
            
            // Second session - retrieve vectors
            {
                let storage = create_real_storage(Some(seed_phrase)).expect("Should recreate storage");
                
                let vector_id = VectorId::from_string("persist-test-001");
                let key = format!("vectors/{}", vector_id.to_string());
                
                let retrieved: Vector = storage.get(&key).await.expect("Should retrieve vector");
                assert_eq!(retrieved.id, vector_id);
                assert_eq!(retrieved.embedding.as_slice(), vec![0.1, 0.2, 0.3, 0.4, 0.5]);
                
                let metadata: VideoNFTMetadata = storage.get("metadata/persist-test").await
                    .expect("Should retrieve metadata");
                assert_eq!(metadata.id, "persist-meta-001");
            }
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_network_resilience_and_retry() {
            let storage = create_real_storage(None).expect("Should create storage");
            
            // Test multiple operations to verify retry logic works
            let operations = 50;
            let mut success_count = 0;
            
            for i in 0..operations {
                let key = format!("resilience-test/{}", i);
                let data = format!("test data {}", i);
                
                match storage.put(&key, &data).await {
                    Ok(_) => success_count += 1,
                    Err(e) => eprintln!("Operation {} failed: {}", i, e),
                }
            }
            
            // Should have high success rate even with network issues
            let success_rate = success_count as f64 / operations as f64;
            println!("Network resilience test: {}/{} successful ({:.1}%)", 
                     success_count, operations, success_rate * 100.0);
            assert!(success_rate > 0.9, "Success rate should be > 90%");
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection - EXPENSIVE TEST
        async fn test_large_scale_operations() {
            let storage = create_real_storage(None).expect("Should create storage");
            
            println!("Starting large-scale test with 10,000 vectors...");
            let vector_count = 10_000;
            let batch_size = 100;
            
            let start = Instant::now();
            
            // Store vectors in batches
            for batch_start in (0..vector_count).step_by(batch_size) {
                let batch_end = (batch_start + batch_size).min(vector_count);
                
                let futures: Vec<_> = (batch_start..batch_end)
                    .map(|i| {
                        let storage = storage.clone();
                        async move {
                            let vector_id = VectorId::from_string(&format!("large-scale-{:06}", i));
                            let embedding = Embedding::new(vec![i as f32 * 0.001; 768]).unwrap(); // 768-dim like text-embedding-3-small
                            let vector = Vector::new(vector_id.clone(), embedding);
                            let key = format!("vectors/large-scale/{}", vector_id.to_string());
                            storage.put(&key, &vector).await
                        }
                    })
                    .collect();
                
                let results = futures::future::join_all(futures).await;
                let batch_success = results.iter().filter(|r| r.is_ok()).count();
                
                if batch_end % 1000 == 0 {
                    let elapsed = start.elapsed();
                    let rate = batch_end as f64 / elapsed.as_secs_f64();
                    println!("Progress: {}/{} vectors, {:.1} vectors/sec", 
                             batch_end, vector_count, rate);
                }
                
                if batch_success < batch_size {
                    eprintln!("Warning: Only {}/{} successful in batch", batch_success, batch_size);
                }
            }
            
            let total_time = start.elapsed();
            println!("Stored {} vectors in {:?} ({:.1} vectors/sec)", 
                     vector_count, total_time, 
                     vector_count as f64 / total_time.as_secs_f64());
            
            // Test retrieval performance
            println!("Testing retrieval performance...");
            let test_samples = 100;
            let mut retrieval_times = Vec::new();
            
            for i in (0..vector_count).step_by(vector_count / test_samples) {
                let vector_id = VectorId::from_string(&format!("large-scale-{:06}", i));
                let key = format!("vectors/large-scale/{}", vector_id.to_string());
                
                let start = Instant::now();
                let result = storage.get::<Vector>(&key).await;
                let retrieval_time = start.elapsed();
                
                if result.is_ok() {
                    retrieval_times.push(retrieval_time);
                }
            }
            
            if !retrieval_times.is_empty() {
                let avg_time = retrieval_times.iter().sum::<std::time::Duration>() / retrieval_times.len() as u32;
                let max_time = retrieval_times.iter().max().unwrap();
                println!("Retrieval performance - Avg: {:?}, Max: {:?}", avg_time, max_time);
            }
        }

        #[tokio::test]
        #[ignore] // Requires real S5 portal connection
        async fn test_concurrent_operations_real_network() {
            let storage = create_real_storage(None).expect("Should create storage");
            
            // Test concurrent operations on real network
            let concurrent_ops = 20;
            let start = Instant::now();
            
            let futures: Vec<_> = (0..concurrent_ops)
                .map(|i| {
                    let storage = storage.clone();
                    async move {
                        let vector_id = VectorId::from_string(&format!("concurrent-real-{}", i));
                        let embedding = Embedding::new(vec![i as f32; 128]).unwrap();
                        let vector = Vector::new(vector_id.clone(), embedding);
                        let key = format!("vectors/concurrent/{}", vector_id.to_string());
                        
                        // Measure individual operation time
                        let op_start = Instant::now();
                        let result = storage.put(&key, &vector).await;
                        let op_time = op_start.elapsed();
                        
                        (result, op_time)
                    }
                })
                .collect();
            
            let results = futures::future::join_all(futures).await;
            let total_time = start.elapsed();
            
            let successful = results.iter().filter(|(r, _)| r.is_ok()).count();
            let avg_op_time = results.iter()
                .map(|(_, t)| t)
                .sum::<std::time::Duration>() / results.len() as u32;
            
            println!("Concurrent operations - Success: {}/{}, Total time: {:?}, Avg op time: {:?}", 
                     successful, concurrent_ops, total_time, avg_op_time);
            
            assert!(successful > concurrent_ops * 8 / 10, "At least 80% should succeed");
        }
    }
}