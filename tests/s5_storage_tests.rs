// tests/s5_storage_tests.rs
// TDD tests for S5 Storage implementation using actual video NFT metadata schema

use vector_db::storage::{Storage, S5Storage, S5Config};
use vector_db::types::{Vector, VideoNFTMetadata, Attribute};
use serde::{Serialize, Deserialize};
use anyhow::Result;

#[tokio::test]
async fn test_s5_storage_creation() {
    // Test creating S5Storage with configuration
    let config = S5Config {
        node_url: "http://localhost:5050".to_string(),
        api_key: Some("test-api-key".to_string()),
        enable_compression: true,
        cache_size: 1000,
    };
    
    let storage = S5Storage::new(config);
    assert!(storage.is_connected().await);
}

#[tokio::test]
async fn test_s5_storage_put_and_get() {
    // Test storing and retrieving data
    let storage = create_test_storage().await;
    
    let key = "test_vector_1";
    let vector = Vector {
        id: "vec1".to_string(),
        values: vec![0.1, 0.2, 0.3, 0.4], // 4-dim test vector
        metadata: Some(serde_json::json!({
            "name": "Test Video",
            "genre": ["Education", "Technology"]
        })),
    };
    
    // Serialize to CBOR
    let cbor_data = vector.to_cbor().expect("CBOR serialization failed");
    
    // Store in S5
    storage.put(key, cbor_data.clone()).await
        .expect("Failed to store in S5");
    
    // Retrieve from S5
    let retrieved_data = storage.get(key).await
        .expect("Failed to retrieve from S5")
        .expect("Data should exist");
    
    assert_eq!(cbor_data, retrieved_data);
    
    // Deserialize and verify
    let retrieved_vector = Vector::from_cbor(&retrieved_data)
        .expect("CBOR deserialization failed");
    
    assert_eq!(vector.id, retrieved_vector.id);
    assert_eq!(vector.values, retrieved_vector.values);
}

#[tokio::test]
async fn test_s5_storage_cid_mapping() {
    // Test that keys are properly mapped to CIDs
    let storage = create_test_storage().await;
    
    let key = "test_key";
    let data = vec![1, 2, 3, 4, 5];
    
    // Store data
    storage.put(key, data.clone()).await.unwrap();
    
    // Get CID for key
    let cid = storage.get_cid(key).await
        .expect("Should have CID for stored key");
    
    // Verify CID format (should be valid S5 CID)
    assert!(cid.starts_with("s5://"));
    
    // Retrieve by CID directly
    let retrieved = storage.get_by_cid(&cid).await
        .expect("Should retrieve by CID");
    
    assert_eq!(data, retrieved);
}

#[tokio::test]
async fn test_s5_storage_delete() {
    // Test deletion
    let storage = create_test_storage().await;
    
    let key = "delete_test";
    let data = vec![1, 2, 3];
    
    // Store and verify exists
    storage.put(key, data).await.unwrap();
    assert!(storage.exists(key).await.unwrap());
    
    // Delete
    storage.delete(key).await.unwrap();
    
    // Verify deleted
    assert!(!storage.exists(key).await.unwrap());
    
    // Attempt to get deleted item should return None
    let result = storage.get(key).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_s5_storage_batch_operations() {
    // Test batch put/get operations
    let storage = create_test_storage().await;
    
    // Prepare batch data
    let batch_data: Vec<(String, Vec<u8>)> = (0..10)
        .map(|i| {
            let key = format!("batch_key_{}", i);
            let data = vec![i as u8; 100]; // 100 bytes each
            (key, data)
        })
        .collect();
    
    // Batch put
    let put_results = storage.batch_put(batch_data.clone()).await
        .expect("Batch put failed");
    
    assert_eq!(put_results.len(), 10);
    assert!(put_results.iter().all(|r| r.is_ok()));
    
    // Batch get
    let keys: Vec<String> = batch_data.iter()
        .map(|(k, _)| k.clone())
        .collect();
    
    let get_results = storage.batch_get(&keys).await
        .expect("Batch get failed");
    
    assert_eq!(get_results.len(), 10);
    
    // Verify data
    for (i, result) in get_results.iter().enumerate() {
        let data = result.as_ref().unwrap();
        assert_eq!(data, &batch_data[i].1);
    }
}

#[tokio::test]
async fn test_s5_storage_error_handling() {
    // Test various error scenarios
    let storage = create_test_storage().await;
    
    // Test getting non-existent key
    let result = storage.get("non_existent").await.unwrap();
    assert!(result.is_none());
    
    // Test invalid CID
    let result = storage.get_by_cid("invalid_cid").await;
    assert!(result.is_err());
    
    // Test network error simulation
    let mut config = S5Config::default();
    config.node_url = "http://invalid-url:9999".to_string();
    let bad_storage = S5Storage::new(config);
    
    let result = bad_storage.put("test", vec![1, 2, 3]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_s5_storage_compression() {
    // Test compression functionality
    let mut config = S5Config::default();
    config.enable_compression = true;
    let storage = S5Storage::new(config);
    
    // Large data that compresses well
    let large_data = vec![42u8; 10000]; // 10KB of same byte
    
    // Store with compression
    let result = storage.put_compressed("compressed_key", large_data.clone()).await;
    assert!(result.is_ok());
    
    // Retrieve and verify decompression
    let retrieved = storage.get("compressed_key").await.unwrap().unwrap();
    assert_eq!(large_data, retrieved);
    
    // Verify actual storage size is smaller (check via S5 metadata)
    let metadata = storage.get_metadata("compressed_key").await.unwrap();
    assert!(metadata.size < 10000); // Should be much smaller due to compression
}

#[tokio::test]
async fn test_s5_storage_concurrent_operations() {
    // Test concurrent reads/writes
    use tokio::task::JoinSet;
    
    let storage = create_test_storage().await;
    let storage = std::sync::Arc::new(storage);
    
    let mut tasks = JoinSet::new();
    
    // Spawn 20 concurrent operations
    for i in 0..20 {
        let storage_clone = storage.clone();
        tasks.spawn(async move {
            let key = format!("concurrent_{}", i);
            let data = vec![i as u8; 1000];
            
            // Write
            storage_clone.put(&key, data.clone()).await.unwrap();
            
            // Read back
            let retrieved = storage_clone.get(&key).await.unwrap().unwrap();
            assert_eq!(data, retrieved);
            
            i
        });
    }
    
    // Wait for all tasks
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result.unwrap());
    }
    
    assert_eq!(results.len(), 20);
}

#[tokio::test]
async fn test_s5_storage_video_nft_metadata() {
    // Test storing and retrieving actual video NFT metadata
    let storage = create_test_storage().await;
    
    let video_metadata = VideoNFTMetadata {
        address: "0xFFbc1e2aFB6ED3d5C1ec98E87a2CB5d1e4aec2a6".to_string(),
        attributes: vec![
            Attribute {
                key: "release_date".to_string(),
                value: serde_json::json!("2022"),
            },
            Attribute {
                key: "runtime".to_string(),
                value: serde_json::json!(125),
            },
            Attribute {
                key: "languages".to_string(),
                value: serde_json::json!(["English"]),
            },
            Attribute {
                key: "countries".to_string(),
                value: serde_json::json!(["United States"]),
            },
        ],
        description: "A woman who raised herself in the marshes of the Deep South becomes a suspect in the murder of a man with whom she was once involved.".to_string(),
        genre: vec!["Drama".to_string(), "Mystery".to_string(), "Romance".to_string()],
        id: "340".to_string(),
        image: "s5://uJh_FJwe3q8Da3NqX1s6end5GKic_uuwDSGk5plBMguNa2RaeDg.jpg".to_string(),
        mintDateTime: "2024-04-04T23:02:43.269Z".to_string(),
        name: "Where the Crawdads Sing".to_string(),
        posterImage: Some("s5://uJh_lUC7lpaMJvixPQtwfCKNbS3m5AXoPe22M6MzG9A6GkTqvCA.jpg".to_string()),
        summary: "A woman who raised herself in the marshes of the Deep South becomes a suspect in the murder of a man with whom she was once involved.".to_string(),
        supply: 1,
        symbol: "MV20".to_string(),
        r#type: "video".to_string(),
        uri: "ipfs://QmaNFjUuUksoBDJpYSm6hv6vkADp7CBzyhWxK6ucw1EUnG".to_string(),
        userPub: "QBg4r4ZzdI5DXtjUTBYol7HiW5EvU7H-Zv64CHei2YU.PHFbYSQ8zKaZYLCb7lD4BePUAs2fJzc171lpZs4zDj4".to_string(),
        video: "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI.mp4".to_string(),
    };
    
    // Store metadata
    let key = format!("metadata_{}_{}",video_metadata.address, video_metadata.id);
    let cbor_data = video_metadata.to_cbor().unwrap();
    
    storage.put(&key, cbor_data).await.unwrap();
    
    // Retrieve and verify
    let retrieved_data = storage.get(&key).await.unwrap().unwrap();
    let retrieved_metadata = VideoNFTMetadata::from_cbor(&retrieved_data).unwrap();
    
    assert_eq!(video_metadata.id, retrieved_metadata.id);
    assert_eq!(video_metadata.name, retrieved_metadata.name);
    assert_eq!(video_metadata.genre, retrieved_metadata.genre);
    assert_eq!(video_metadata.address, retrieved_metadata.address);
    
    // Verify runtime from attributes
    let runtime_attr = retrieved_metadata.attributes.iter()
        .find(|a| a.key == "runtime")
        .unwrap();
    assert_eq!(runtime_attr.value, 125);
}

#[tokio::test]
async fn test_s5_storage_multiple_video_types() {
    // Test different NFT types (video, audio, image, data)
    let storage = create_test_storage().await;
    
    let nft_types = vec!["video", "audio", "image", "data"];
    
    for nft_type in nft_types {
        let metadata = VideoNFTMetadata {
            address: "0x742d35Cc6634C0532925a3b844Bc9e7595f06789".to_string(),
            id: format!("test_{}", nft_type),
            name: format!("Test {} NFT", nft_type),
            r#type: nft_type.to_string(),
            // ... other fields with default values
            ..Default::default()
        };
        
        let key = format!("{}_{}", nft_type, metadata.id);
        let cbor_data = metadata.to_cbor().unwrap();
        
        storage.put(&key, cbor_data).await.unwrap();
        
        // Verify retrieval
        let retrieved = storage.get(&key).await.unwrap().unwrap();
        let decoded = VideoNFTMetadata::from_cbor(&retrieved).unwrap();
        
        assert_eq!(decoded.r#type, nft_type);
    }
}

// Helper function to create test storage
async fn create_test_storage() -> S5Storage {
    let config = S5Config {
        node_url: std::env::var("S5_TEST_NODE_URL")
            .unwrap_or_else(|_| "http://localhost:5050".to_string()),
        api_key: std::env::var("S5_TEST_API_KEY").ok(),
        enable_compression: false,
        cache_size: 100,
    };
    
    S5Storage::new(config)
}