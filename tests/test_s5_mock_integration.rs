// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

// tests/test_s5_mock_integration.rs
// Phase 8.2 - Comprehensive Mock Server Integration Tests

use vector_db::storage::{Storage, S5StorageAdapter, S5StorageConfig, StorageMode, EnhancedS5Storage};
use vector_db::core::{Vector, VectorId, Embedding};
use vector_db::types::{VideoNFTMetadata, Attribute};
use std::time::Instant;
use futures::stream::{FuturesUnordered, StreamExt};
// futures::future::join_all is imported but not used since we use join3 instead

/// Helper to check if mock server is available
async fn is_mock_server_available() -> bool {
    // When in Docker, try container name first
    let urls = if std::path::Path::new("/.dockerenv").exists() {
        vec![
            "http://s5-mock:5524/health",          // Container name (preferred)
            "http://host.docker.internal:5524/health", // Fallback
        ]
    } else {
        vec!["http://localhost:5524/health"]      // Host machine
    };
    
    for url in urls {
        if let Ok(response) = reqwest::get(url).await {
            if response.status().is_success() {
                return true;
            }
        }
    }
    false
}

/// Create a test storage instance configured for mock server with Docker support
fn create_mock_storage() -> Result<EnhancedS5Storage, Box<dyn std::error::Error + Send + Sync>> {
    let mock_server_url = if std::path::Path::new("/.dockerenv").exists() {
        "http://s5-mock:5524".to_string()  // Use container name when in Docker
    } else {
        "http://localhost:5524".to_string()
    };
    
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some(mock_server_url),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
    };
    
    EnhancedS5Storage::new(config)
}

mod phase_8_2_mock_server_integration {
    use super::*;

    mod vector_crud_operations {
        use super::*;

        #[tokio::test]
        async fn test_vector_store_and_retrieve() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available at http://localhost:5524");
                return;
            }

            let storage = create_mock_storage().unwrap();
            
            // Create a test vector
            let vector_id = VectorId::from_string("mock-test-vector-001");
            let embedding = Embedding::new(vec![0.1, 0.2, 0.3, 0.4, 0.5]).unwrap();
            let vector = Vector::new(vector_id.clone(), embedding.clone());

            // Store the vector
            let key = format!("vectors/{}", vector_id.to_string());
            <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.expect("Failed to store vector");

            // Retrieve and verify
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await.expect("Failed to retrieve vector");
            assert_eq!(retrieved.id, vector_id);
            assert_eq!(retrieved.embedding.as_slice(), embedding.as_slice());
        }

        #[tokio::test]
        async fn test_vector_update() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let vector_id = VectorId::from_string("mock-test-vector-update");
            
            // Store initial vector
            let initial_embedding = Embedding::new(vec![1.0, 2.0, 3.0]).unwrap();
            let initial_vector = Vector::new(vector_id.clone(), initial_embedding);
            let key = format!("vectors/{}", vector_id.to_string());
            <EnhancedS5Storage as Storage>::put(&storage, &key, &initial_vector).await.unwrap();

            // Update with new embedding
            let updated_embedding = Embedding::new(vec![4.0, 5.0, 6.0]).unwrap();
            let updated_vector = Vector::new(vector_id.clone(), updated_embedding.clone());
            <EnhancedS5Storage as Storage>::put(&storage, &key, &updated_vector).await.unwrap();

            // Verify update
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
            assert_eq!(retrieved.embedding.as_slice(), updated_embedding.as_slice());
        }

        #[tokio::test]
        async fn test_vector_delete() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let vector_id = VectorId::from_string("mock-test-vector-delete");
            let embedding = Embedding::new(vec![1.0, 2.0, 3.0]).unwrap();
            let vector = Vector::new(vector_id.clone(), embedding);
            let key = format!("vectors/{}", vector_id.to_string());

            // Store, verify exists, delete, verify gone
            <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.unwrap();
            assert!(<EnhancedS5Storage as Storage>::exists(&storage, &key).await.unwrap());
            
            <EnhancedS5Storage as Storage>::delete(&storage, &key).await.unwrap();
            assert!(!<EnhancedS5Storage as Storage>::exists(&storage, &key).await.unwrap());
        }

        #[tokio::test]
        async fn test_vector_with_metadata() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let vector_id = VectorId::from_string("mock-test-vector-metadata");
            let embedding = Embedding::new(vec![0.1, 0.2, 0.3]).unwrap();
            
            let metadata = serde_json::json!({
                "video_id": "video_123",
                "title": "Test Video",
                "tags": ["AI", "tutorial"],
                "duration_seconds": 300
            });
            
            let mut vector = Vector::new(vector_id.clone(), embedding);
            vector.metadata = Some(metadata.clone());
            let key = format!("vectors/{}", vector_id.to_string());

            <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.unwrap();
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
            
            assert_eq!(retrieved.metadata, Some(metadata));
        }
        
        #[tokio::test]
        async fn test_single_vector_crud_with_timing() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            assert!(<EnhancedS5Storage as S5StorageAdapter>::is_connected(&storage).await, "Should be connected to mock server");
            
            // Create a test vector
            let vector_id = VectorId::from_string("test-vector-crud-timing");
            let embedding = Embedding::new(vec![0.1, 0.2, 0.3, 0.4, 0.5]).unwrap();
            let vector = Vector::new(vector_id.clone(), embedding);
            
            // PUT operation
            let start = Instant::now();
            <EnhancedS5Storage as Storage>::put(&storage, &format!("vectors/{}", vector_id.to_string()), &vector).await.unwrap();
            let put_duration = start.elapsed();
            println!("PUT duration: {:?}", put_duration);
            assert!(put_duration.as_millis() < 100, "PUT should complete within 100ms");
            
            // EXISTS operation
            let start = Instant::now();
            let exists = <EnhancedS5Storage as Storage>::exists(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            let exists_duration = start.elapsed();
            println!("EXISTS duration: {:?}", exists_duration);
            assert!(exists, "Vector should exist after PUT");
            assert!(exists_duration.as_millis() < 50, "EXISTS should complete within 50ms");
            
            // GET operation
            let start = Instant::now();
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            let get_duration = start.elapsed();
            println!("GET duration: {:?}", get_duration);
            assert_eq!(retrieved.id, vector_id, "Retrieved vector ID should match");
            assert_eq!(retrieved.embedding.as_slice(), vector.embedding.as_slice(), "Retrieved embedding should match");
            assert!(get_duration.as_millis() < 100, "GET should complete within 100ms");
            
            // DELETE operation
            let start = Instant::now();
            <EnhancedS5Storage as Storage>::delete(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            let delete_duration = start.elapsed();
            println!("DELETE duration: {:?}", delete_duration);
            assert!(delete_duration.as_millis() < 100, "DELETE should complete within 100ms");
            
            // Verify deletion
            let exists_after = <EnhancedS5Storage as Storage>::exists(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            assert!(!exists_after, "Vector should not exist after DELETE");
        }
        
        #[tokio::test]
        async fn test_complex_metadata_crud() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            // Create complex metadata
            let metadata = VideoNFTMetadata {
                address: "0x742d35Cc6634C0532925a3b844Bc9e7595f4a9e0".to_string(),
                id: "nft-complex-001".to_string(),
                name: "Advanced AI Tutorial: Deep Learning Fundamentals".to_string(),
                image: "https://example.com/thumbnails/ai-tutorial-001.jpg".to_string(),
                animation_url: Some("https://example.com/videos/ai-tutorial-001.mp4".to_string()),
                mint_date_time: chrono::Utc::now(),
                genre: vec!["AI".to_string(), "Tutorial".to_string(), "Education".to_string()],
                r#type: "video".to_string(),
                attributes: vec![
                    Attribute {
                        key: "Duration".to_string(),
                        value: serde_json::json!("45:30"),
                    },
                    Attribute {
                        key: "Quality".to_string(),
                        value: serde_json::json!("4K"),
                    },
                    Attribute {
                        key: "Language".to_string(),
                        value: serde_json::json!("English"),
                    },
                ],
                description: Some("A comprehensive tutorial on deep learning fundamentals, covering neural networks, backpropagation, and modern architectures.".to_string()),
                poster_image: Some("https://example.com/posters/ai-tutorial-001.jpg".to_string()),
                summary: Some("Learn the foundations of deep learning in this 45-minute tutorial.".to_string()),
                supply: Some(1),
                symbol: Some("AITUT001".to_string()),
                uri: Some("https://example.com/metadata/ai-tutorial-001.json".to_string()),
                user_pub: Some("0x1234567890abcdef".to_string()),
                video: Some("https://ipfs.io/ipfs/QmXyz...".to_string()),
            };
            
            // Store and retrieve
            let key = format!("metadata/{}", metadata.id);
            <EnhancedS5Storage as Storage>::put(&storage, &key, &metadata).await.unwrap();
            
            let retrieved: VideoNFTMetadata = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
            
            // Verify all fields
            assert_eq!(retrieved.id, metadata.id);
            assert_eq!(retrieved.name, metadata.name);
            assert_eq!(retrieved.genre, metadata.genre);
            assert_eq!(retrieved.attributes.len(), metadata.attributes.len());
            assert_eq!(retrieved.attributes[0].key, "Duration");
            assert_eq!(retrieved.description, metadata.description);
            assert_eq!(retrieved.symbol, metadata.symbol);
            
            // Clean up
            <EnhancedS5Storage as Storage>::delete(&storage, &key).await.unwrap();
        }
    }

    mod batch_operations {
        use super::*;

        #[tokio::test]
        async fn test_batch_vector_operations() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let batch_size = 100;
            
            // Create batch of vectors
            let vectors: Vec<Vector> = (0..batch_size)
                .map(|i| {
                    let id = VectorId::from_string(&format!("batch-vector-{:04}", i));
                    let embedding = Embedding::new(vec![i as f32 * 0.1; 128]).unwrap();
                    Vector::new(id, embedding)
                })
                .collect();

            let start = Instant::now();
            
            // Parallel batch insert
            let mut futures = FuturesUnordered::new();
            for vector in &vectors {
                let storage = storage.clone();
                let key = format!("vectors/{}", vector.id.to_string());
                let vector = vector.clone();
                futures.push(async move {
                    <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await
                });
            }
            
            while let Some(result) = futures.next().await {
                result.expect("Failed to insert vector");
            }
            
            let insert_time = start.elapsed();
            println!("Batch insert of {} vectors took: {:?}", batch_size, insert_time);
            
            // Verify all vectors exist
            let mut verify_futures = FuturesUnordered::new();
            for vector in &vectors {
                let storage = storage.clone();
                let key = format!("vectors/{}", vector.id.to_string());
                verify_futures.push(async move {
                    <EnhancedS5Storage as Storage>::exists(&storage, &key).await
                });
            }
            
            let mut exist_count = 0;
            while let Some(result) = verify_futures.next().await {
                if result.unwrap() {
                    exist_count += 1;
                }
            }
            
            assert_eq!(exist_count, batch_size);
        }

        #[tokio::test]
        async fn test_concurrent_operations() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let concurrent_ops = 50;
            
            // Mix of operations: insert, update, read
            let mut futures = FuturesUnordered::new();
            
            for i in 0..concurrent_ops {
                let storage = storage.clone();
                futures.push(async move {
                    let vector_id = VectorId::from_string(&format!("concurrent-{}", i));
                    let embedding = Embedding::new(vec![i as f32; 64]).unwrap();
                    let vector = Vector::new(vector_id.clone(), embedding);
                    let key = format!("vectors/{}", vector_id.to_string());
                    
                    // Insert
                    <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
                    
                    // Read
                    let _retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
                    
                    // Update
                    let updated_embedding = Embedding::new(vec![i as f32 * 2.0; 64]).unwrap();
                    let updated_vector = Vector::new(vector_id, updated_embedding);
                    <EnhancedS5Storage as Storage>::put(&storage, &key, &updated_vector).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
                    
                    Ok::<(), Box<dyn std::error::Error>>(())
                });
            }
            
            let mut success_count = 0;
            while let Some(result) = futures.next().await {
                if result.is_ok() {
                    success_count += 1;
                }
            }
            
            assert_eq!(success_count, concurrent_ops);
        }
        
        #[tokio::test]
        async fn test_mixed_type_batch_operations() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            // Store different types of data concurrently
            let storage_clone1 = storage.clone();
            let storage_clone2 = storage.clone();
            let storage_clone3 = storage.clone();
            
            let fut1 = async move {
                let vector = Vector::new(
                    VectorId::from_string("mixed-vector-001"),
                    Embedding::new(vec![0.1, 0.2, 0.3]).unwrap()
                );
                <EnhancedS5Storage as Storage>::put(&storage_clone1, "mixed/vector/001", &vector).await
            };
            
            let fut2 = async move {
                let metadata = VideoNFTMetadata {
                    address: "0xabc".to_string(),
                    id: "mixed-nft-001".to_string(),
                    name: "Mixed Test Video".to_string(),
                    image: "https://example.com/mixed.jpg".to_string(),
                    animation_url: None,
                    mint_date_time: chrono::Utc::now(),
                    genre: vec!["Test".to_string()],
                    r#type: "video".to_string(),
                    attributes: vec![],
                    description: None,
                    poster_image: None,
                    summary: None,
                    supply: None,
                    symbol: None,
                    uri: None,
                    user_pub: None,
                    video: None,
                };
                <EnhancedS5Storage as Storage>::put(&storage_clone2, "mixed/metadata/001", &metadata).await
            };
            
            let fut3 = async move {
                let raw_data = vec![1u8, 2, 3, 4, 5];
                <EnhancedS5Storage as S5StorageAdapter>::put_raw(&storage_clone3, "mixed/raw/001", raw_data).await
            };
            
            let (r1, r2, r3) = futures::future::join3(fut1, fut2, fut3).await;
            r1.unwrap();
            r2.unwrap();
            r3.unwrap();
            
            // Verify all data exists
            assert!(<EnhancedS5Storage as Storage>::exists(&storage, "mixed/vector/001").await.unwrap());
            assert!(<EnhancedS5Storage as Storage>::exists(&storage, "mixed/metadata/001").await.unwrap());
            assert!(<EnhancedS5Storage as Storage>::exists(&storage, "mixed/raw/001").await.unwrap());
            
            // List all mixed data
            let listed = <EnhancedS5Storage as Storage>::list(&storage, "mixed/").await.unwrap();
            assert!(listed.len() >= 3, "Should list at least 3 items");
            
            // Cleanup
            <EnhancedS5Storage as Storage>::delete(&storage, "mixed/vector/001").await.unwrap();
            <EnhancedS5Storage as Storage>::delete(&storage, "mixed/metadata/001").await.unwrap();
            <EnhancedS5Storage as Storage>::delete(&storage, "mixed/raw/001").await.unwrap();
        }
    }

    mod hamt_sharding {
        use super::*;

        #[tokio::test]
        #[ignore] // This test is expensive - run with: cargo test test_hamt_activation -- --ignored
        async fn test_hamt_activation_at_threshold() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let _hamt_threshold = 1000;
            let vectors_to_create = 1100; // Just over threshold
            
            println!("Creating {} vectors to trigger HAMT sharding...", vectors_to_create);
            
            // Create vectors in batches to avoid overwhelming the system
            let batch_size = 100;
            for batch_start in (0..vectors_to_create).step_by(batch_size) {
                let batch_end = (batch_start + batch_size).min(vectors_to_create);
                
                let mut batch_futures = FuturesUnordered::new();
                for i in batch_start..batch_end {
                    let storage = storage.clone();
                    let vector_id = VectorId::from_string(&format!("hamt-test-{:06}", i));
                    let embedding = Embedding::new(vec![i as f32 * 0.001; 256]).unwrap();
                    let vector = Vector::new(vector_id.clone(), embedding);
                    let key = format!("vectors/hamt/{}", vector_id.to_string());
                    
                    batch_futures.push(async move {
                        <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await
                    });
                }
                
                while let Some(result) = batch_futures.next().await {
                    result.expect("Failed to insert vector for HAMT test");
                }
                
                if batch_end % 200 == 0 {
                    println!("Progress: {}/{} vectors created", batch_end, vectors_to_create);
                }
            }
            
            // Test retrieval performance after HAMT activation
            let test_indices = vec![0, 500, 999, 1000, 1050, 1099];
            for i in test_indices {
                let start = Instant::now();
                let vector_id = VectorId::from_string(&format!("hamt-test-{:06}", i));
                let key = format!("vectors/hamt/{}", vector_id.to_string());
                
                let _vector: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await
                    .expect(&format!("Failed to retrieve vector at index {}", i));
                
                let retrieval_time = start.elapsed();
                println!("Retrieved vector {} in {:?}", i, retrieval_time);
                
                // Even with HAMT, retrieval should be fast
                assert!(retrieval_time.as_millis() < 100, 
                    "Retrieval took too long: {:?}", retrieval_time);
            }
        }

        #[tokio::test]
        async fn test_directory_listing() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            let prefix = "vectors/listing-test";
            let count = 50;
            
            // Create vectors with specific prefix
            for i in 0..count {
                let vector_id = VectorId::from_string(&format!("listing-{:03}", i));
                let embedding = Embedding::new(vec![i as f32; 32]).unwrap();
                let vector = Vector::new(vector_id.clone(), embedding);
                let key = format!("{}/{}", prefix, vector_id.to_string());
                <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.unwrap();
            }
            
            // List directory
            let entries = <EnhancedS5Storage as Storage>::list(&storage, prefix).await.unwrap();
            assert_eq!(entries.len(), count);
            
            // Verify entries are properly named
            for (i, entry) in entries.iter().enumerate() {
                assert!(entry.contains(&format!("listing-{:03}", i)));
            }
        }
    }

    mod metadata_operations {
        use super::*;

        #[tokio::test]
        async fn test_video_metadata_storage() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            
            let metadata = VideoNFTMetadata {
                address: "0x1234567890abcdef".to_string(),
                id: "video-nft-001".to_string(),
                name: "AI Tutorial Video".to_string(),
                image: "https://example.com/thumbnail.jpg".to_string(),
                animation_url: Some("https://example.com/video.mp4".to_string()),
                mint_date_time: chrono::Utc::now(),
                genre: vec!["AI".to_string(), "Tutorial".to_string(), "Educational".to_string()],
                r#type: "video".to_string(),
                attributes: vec![
                    Attribute {
                        key: "Duration".to_string(),
                        value: serde_json::json!("15:30"),
                    },
                    Attribute {
                        key: "Language".to_string(),
                        value: serde_json::json!("English"),
                    }
                ],
                description: Some("Comprehensive AI tutorial".to_string()),
                poster_image: Some("https://example.com/poster.jpg".to_string()),
                summary: Some("Learn AI fundamentals".to_string()),
                supply: Some(1000),
                symbol: Some("AITUT".to_string()),
                uri: Some("https://example.com/metadata.json".to_string()),
                user_pub: Some("user123".to_string()),
                video: Some("https://ipfs.io/ipfs/QmVideo123".to_string()),
            };
            
            let key = format!("metadata/videos/{}", metadata.id);
            <EnhancedS5Storage as Storage>::put(&storage, &key, &metadata).await.unwrap();
            
            let retrieved: VideoNFTMetadata = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
            assert_eq!(retrieved.id, metadata.id);
            assert_eq!(retrieved.genre, metadata.genre);
            assert_eq!(retrieved.attributes.len(), 2);
        }

        #[tokio::test]
        async fn test_complex_cbor_serialization() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            
            // Test with various data types
            let complex_data = serde_json::json!({
                "nested": {
                    "arrays": vec![1, 2, 3, 4, 5],
                    "floats": vec![1.1, 2.2, 3.3],
                    "booleans": vec![true, false, true],
                    "null_value": null,
                    "empty_array": Vec::<i32>::new(),
                    "unicode": "Hello 世界 🌍",
                    "large_number": i64::MAX,
                    "negative": -42
                },
                "metadata": {
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "tags": ["test", "cbor", "serialization"]
                }
            });
            
            let key = "test/complex-cbor-data";
            <EnhancedS5Storage as Storage>::put(&storage, &key, &complex_data).await.unwrap();
            
            let retrieved: serde_json::Value = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
            assert_eq!(retrieved["nested"]["arrays"], complex_data["nested"]["arrays"]);
            assert_eq!(retrieved["nested"]["unicode"], complex_data["nested"]["unicode"]);
            assert_eq!(retrieved["metadata"]["tags"], complex_data["metadata"]["tags"]);
        }
    }

    mod performance_tests {
        use super::*;

        #[tokio::test]
        async fn test_retrieval_performance() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }

            let storage = create_mock_storage().unwrap();
            
            // Store test vectors
            let count = 100;
            for i in 0..count {
                let vector_id = VectorId::from_string(&format!("perf-test-{:03}", i));
                let embedding = Embedding::new(vec![i as f32; 128]).unwrap();
                let vector = Vector::new(vector_id.clone(), embedding);
                let key = format!("vectors/performance/{}", vector_id.to_string());
                <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await.unwrap();
            }
            
            // Measure retrieval times
            let mut total_time = std::time::Duration::ZERO;
            let iterations = 50;
            
            for i in 0..iterations {
                let vector_id = VectorId::from_string(&format!("perf-test-{:03}", i % count));
                let key = format!("vectors/performance/{}", vector_id.to_string());
                
                let start = Instant::now();
                let _vector: Vector = <EnhancedS5Storage as Storage>::get(&storage, &key).await.unwrap();
                total_time += start.elapsed();
            }
            
            let avg_time = total_time / iterations as u32;
            println!("Average retrieval time: {:?}", avg_time);
            
            // Should be fast with caching
            assert!(avg_time.as_millis() < 50, 
                "Average retrieval time too high: {:?}", avg_time);
        }
        
        #[tokio::test]
        async fn test_caching_effectiveness() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            let vector = Vector::new(
                VectorId::from_string("cache-test-001"),
                Embedding::new(vec![0.1; 256]).unwrap()
            );
            
            // Store vector
            <EnhancedS5Storage as Storage>::put(&storage, "cache/vector/001", &vector).await.unwrap();
            
            // First GET (cold cache)
            let start = Instant::now();
            let _: Vector = <EnhancedS5Storage as Storage>::get(&storage, "cache/vector/001").await.unwrap();
            let cold_duration = start.elapsed();
            println!("Cold cache GET duration: {:?}", cold_duration);
            
            // Second GET (warm cache)
            let start = Instant::now();
            let _: Vector = <EnhancedS5Storage as Storage>::get(&storage, "cache/vector/001").await.unwrap();
            let warm_duration = start.elapsed();
            println!("Warm cache GET duration: {:?}", warm_duration);
            
            // Cache should significantly improve performance
            assert!(warm_duration < cold_duration / 2, 
                "Cached GET should be at least 2x faster than cold GET");
            assert!(warm_duration.as_micros() < 1000, 
                "Cached GET should complete within 1ms");
            
            // Verify stats show cache usage
            let stats = <EnhancedS5Storage as S5StorageAdapter>::get_stats(&storage).await.unwrap();
            let cache_entries = stats["cache_entries"].as_u64().unwrap();
            assert!(cache_entries > 0, "Stats should show cache entries");
            
            // Cleanup
            <EnhancedS5Storage as Storage>::delete(&storage, "cache/vector/001").await.unwrap();
        }
        
        #[tokio::test]
        async fn test_large_data_handling() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            // Create large vector (10K dimensions)
            let large_embedding = Embedding::new(vec![0.1; 10_000]).unwrap();
            let large_vector = Vector::new(
                VectorId::from_string("large-vector-001"),
                large_embedding
            );
            
            // Store and retrieve
            let start = Instant::now();
            <EnhancedS5Storage as Storage>::put(&storage, "large/vector/001", &large_vector).await.unwrap();
            let put_duration = start.elapsed();
            println!("Large vector PUT duration: {:?}", put_duration);
            
            let start = Instant::now();
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, "large/vector/001").await.unwrap();
            let get_duration = start.elapsed();
            println!("Large vector GET duration: {:?}", get_duration);
            
            assert_eq!(retrieved.embedding.as_slice().len(), 10_000);
            assert!(put_duration.as_millis() < 500, "Large PUT should complete within 500ms");
            assert!(get_duration.as_millis() < 300, "Large GET should complete within 300ms");
            
            // Cleanup
            <EnhancedS5Storage as Storage>::delete(&storage, "large/vector/001").await.unwrap();
        }
    }
    
    mod error_handling_and_resilience {
        use super::*;
        
        #[tokio::test]
        async fn test_retry_mechanism() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            // Create storage with aggressive retry settings
            let mock_server_url = if std::path::Path::new("/.dockerenv").exists() {
                "http://s5-mock:5524".to_string()
            } else {
                "http://localhost:5524".to_string()
            };
            
            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some(mock_server_url),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: Some(1000), // 1 second timeout
                retry_attempts: Some(3),
            };
            
            let storage = EnhancedS5Storage::new(config).unwrap();
            
            // Test operations still succeed with retries
            let vector = Vector::new(
                VectorId::from_string("retry-test-001"),
                Embedding::new(vec![0.1, 0.2, 0.3]).unwrap()
            );
            
            <EnhancedS5Storage as Storage>::put(&storage, "retry/vector/001", &vector).await.unwrap();
            let exists = <EnhancedS5Storage as Storage>::exists(&storage, "retry/vector/001").await.unwrap();
            assert!(exists);
            
            <EnhancedS5Storage as Storage>::delete(&storage, "retry/vector/001").await.unwrap();
        }
        
        #[tokio::test]
        async fn test_nonexistent_key_handling() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            // Test GET on non-existent key
            let result = <EnhancedS5Storage as Storage>::get::<Vector>(&storage, "nonexistent/key").await;
            assert!(result.is_err(), "GET on non-existent key should fail");
            
            // Test EXISTS on non-existent key
            let exists = <EnhancedS5Storage as Storage>::exists(&storage, "nonexistent/key").await.unwrap();
            assert!(!exists, "EXISTS should return false for non-existent key");
            
            // Test DELETE on non-existent key (should succeed)
            <EnhancedS5Storage as Storage>::delete(&storage, "nonexistent/key").await.unwrap();
        }
        
        #[tokio::test]
        async fn test_invalid_data_handling() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            let storage = create_mock_storage().unwrap();
            
            // Store raw data
            let raw_data = b"This is not valid CBOR data for a Vector".to_vec();
            <EnhancedS5Storage as S5StorageAdapter>::put_raw(&storage, "invalid/vector", raw_data).await.unwrap();
            
            // Try to deserialize as Vector
            let result = <EnhancedS5Storage as Storage>::get::<Vector>(&storage, "invalid/vector").await;
            assert!(result.is_err(), "Should fail to deserialize invalid data");
            
            // Cleanup
            <EnhancedS5Storage as Storage>::delete(&storage, "invalid/vector").await.unwrap();
        }
    }
    
    mod docker_specific_tests {
        use super::*;
        
        #[tokio::test]
        async fn test_docker_networking_detection() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available");
                return;
            }
            
            // Test that storage correctly detects Docker environment
            let storage = create_mock_storage().unwrap();
            let stats = <EnhancedS5Storage as S5StorageAdapter>::get_stats(&storage).await.unwrap();
            let base_url = stats["base_url"].as_str().unwrap();
            
            let in_docker = std::path::Path::new("/.dockerenv").exists() ||
                std::fs::read_to_string("/proc/1/cgroup")
                    .unwrap_or_default()
                    .contains("docker");
            
            if in_docker {
                assert!(base_url.contains("s5-mock"), 
                    "Should use s5-mock container name when running in Docker");
            } else {
                assert!(base_url.contains("localhost"), 
                    "Should use localhost when not in Docker");
            }
            
            // Verify connection works regardless
            assert!(<EnhancedS5Storage as S5StorageAdapter>::is_connected(&storage).await);
        }
    }
}