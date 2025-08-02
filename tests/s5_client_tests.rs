// tests/s5_client_tests.rs
// TDD tests for S5 client integration with enhanced s5.js

use vector_db::storage::{S5Client, S5Config};
use vector_db::types::Vector;

#[tokio::test]
async fn test_s5_client_creation() {
    // Test creating S5 client with configuration
    let config = S5Config {
        node_url: "http://localhost:5524".to_string(), // Default S5 node port
        api_key: Some("test-api-key".to_string()),
        enable_compression: true,
        cache_size: 100,
    };
    
    let _client = S5Client::new(config);
    // Note: Can't test health_check without mock or real server
    // Just ensure client is created successfully
}

#[tokio::test]
async fn test_s5_client_upload_data() {
    // Test uploading data to S5 network
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock S5 upload endpoint
    let _m = server.mock("POST", "/s5/upload")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"cid":"s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI"}"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let data = b"Hello, S5!";
    
    let cid = client.upload_data(data.to_vec()).await.unwrap();
    assert!(cid.starts_with("s5://"));
}

#[tokio::test]
async fn test_s5_client_download_data() {
    // Test downloading data from S5 network
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    let test_cid = "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI";
    let test_data = b"Hello from S5!";
    
    // Mock S5 download endpoint
    let _m = server.mock("GET", "/s5/download/uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI")
        .with_status(200)
        .with_body(test_data)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let downloaded = client.download_data(test_cid).await.unwrap();
    
    assert_eq!(downloaded, test_data);
}

#[tokio::test]
async fn test_s5_client_path_based_api() {
    // Test using enhanced s5.js path-based API
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock path-based PUT
    let _m1 = server.mock("PUT", "/s5/fs/vectors/embeddings/video_123.cbor")
        .match_header("content-type", "application/cbor")
        .with_status(200)
        .with_body(r#"{"cid":"s5://test_cid_123","path":"vectors/embeddings/video_123.cbor"}"#)
        .create_async()
        .await;
    
    // Mock path-based GET
    let _m2 = server.mock("GET", "/s5/fs/vectors/embeddings/video_123.cbor")
        .with_status(200)
        .with_header("content-type", "application/cbor")
        .with_body(vec![0x84, 0x66, 0x53, 0x35]) // Sample CBOR data
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    
    // Test path-based upload
    let path = "vectors/embeddings/video_123.cbor";
    let data = vec![1, 2, 3, 4];
    let result = client.put_path(path, data.clone()).await.unwrap();
    assert!(result.cid.starts_with("s5://"));
    
    // Test path-based download
    let downloaded = client.get_path(path).await.unwrap();
    assert!(!downloaded.is_empty());
}

#[tokio::test]
async fn test_s5_client_list_directory() {
    // Test listing directory contents
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock directory listing
    let _m = server.mock("GET", "/s5/fs/vectors/embeddings/")
        .with_status(200)
        .with_body(r#"{
            "entries": [
                {"name": "video_123.cbor", "type": "file", "size": 3456},
                {"name": "video_456.cbor", "type": "file", "size": 3512},
                {"name": "metadata", "type": "directory"}
            ],
            "cursor": null
        }"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let entries = client.list_path("vectors/embeddings").await.unwrap();
    
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "video_123.cbor");
    assert_eq!(entries[0].entry_type, "file");
}

#[tokio::test]
async fn test_s5_client_error_handling() {
    // Test various error scenarios
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock 404 error
    let _m = server.mock("GET", "/s5/download/invalid_cid")
        .with_status(404)
        .with_body(r#"{"error":"CID not found"}"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let result = client.download_data("s5://invalid_cid").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("404") || error.to_string().contains("not found"));
}

#[tokio::test]
async fn test_s5_client_with_auth() {
    // Test authenticated operations
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: Some("secret-key".to_string()),
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock authenticated upload
    let _m = server.mock("POST", "/s5/upload")
        .match_header("authorization", "Bearer secret-key")
        .with_status(200)
        .with_body(r#"{"cid":"s5://authenticated_upload"}"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let cid = client.upload_data(b"authenticated data".to_vec()).await.unwrap();
    
    assert_eq!(cid, "s5://authenticated_upload");
}

#[tokio::test]
async fn test_s5_client_batch_operations() {
    // Test batch upload/download
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // Mock individual uploads for batch operation
    let _m1 = server.mock("PUT", "/s5/fs/vec1.cbor")
        .match_header("content-type", "application/cbor")
        .with_status(200)
        .with_body(r#"{"cid":"s5://cid1","path":"vec1.cbor"}"#)
        .create_async()
        .await;
    
    let _m2 = server.mock("PUT", "/s5/fs/vec2.cbor")
        .match_header("content-type", "application/cbor")
        .with_status(200)
        .with_body(r#"{"cid":"s5://cid2","path":"vec2.cbor"}"#)
        .create_async()
        .await;
    
    let _m3 = server.mock("PUT", "/s5/fs/vec3.cbor")
        .match_header("content-type", "application/cbor")
        .with_status(200)
        .with_body(r#"{"cid":"s5://cid3","path":"vec3.cbor"}"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    
    let batch_data = vec![
        ("vec1.cbor", vec![1, 2, 3]),
        ("vec2.cbor", vec![4, 5, 6]),
        ("vec3.cbor", vec![7, 8, 9]),
    ];
    
    let results = client.batch_upload(batch_data).await.unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.success));
}

#[tokio::test]
async fn test_s5_client_metadata_operations() {
    // Test getting metadata for stored data
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    let test_cid = "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI";
    
    // Mock metadata endpoint
    let _m = server.mock("GET", "/s5/metadata/uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI")
        .with_status(200)
        .with_body(r#"{
            "cid": "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI",
            "size": 3456,
            "mime_type": "application/cbor",
            "created_at": 1705745000,
            "encryption": null
        }"#)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let metadata = client.get_metadata(test_cid).await.unwrap();
    
    assert_eq!(metadata.size, 3456);
    assert_eq!(metadata.mime_type, "application/cbor");
}

#[tokio::test]
async fn test_s5_client_retry_logic() {
    // Test retry on transient failures
    let mut server = mockito::Server::new_async().await;
    let mock_url = server.url();
    
    let config = S5Config {
        node_url: mock_url.clone(),
        api_key: None,
        enable_compression: false,
        cache_size: 0,
    };
    
    // First two calls fail, third succeeds
    let _m1 = server.mock("POST", "/s5/upload")
        .with_status(503)
        .expect_at_least(2)
        .create_async()
        .await;
    
    let _m2 = server.mock("POST", "/s5/upload")
        .with_status(200)
        .with_body(r#"{"cid":"s5://retry_success"}"#)
        .expect_at_least(1)
        .create_async()
        .await;
    
    let client = S5Client::new(config);
    let result = client.upload_data_with_retry(b"retry test".to_vec()).await;
    
    // Should succeed after retries
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "s5://retry_success");
}

// Test with real S5 node (requires local S5 node running)
#[tokio::test]
#[ignore = "requires local S5 node"]
async fn test_s5_client_real_node() {
    let config = S5Config {
        node_url: "http://localhost:5524".to_string(),
        api_key: None,
        enable_compression: true,
        cache_size: 100,
    };
    
    let client = S5Client::new(config);
    
    // Test real upload
    let vector = Vector {
        id: "test_real_vec".to_string(),
        values: vec![0.1, 0.2, 0.3],
        metadata: None,
    };
    
    let cbor_data = vector.to_cbor().unwrap();
    
    // Use path-based API for upload
    let path = "vectors/test_real_vec.cbor";
    let result = client.put_path(path, cbor_data.clone()).await.unwrap();
    let cid = result.cid;
    
    println!("Uploaded to S5 at path {}: {}", path, cid);
    
    // Test real download using path-based API
    let downloaded = client.get_path(path).await.unwrap();
    assert_eq!(cbor_data, downloaded);
    
    // Verify vector integrity
    let decoded = Vector::from_cbor(&downloaded).unwrap();
    assert_eq!(vector.id, decoded.id);
    assert_eq!(vector.values, decoded.values);
    
    // Test directory listing to verify file exists
    let entries = client.list_path("vectors").await.unwrap();
    println!("Directory listing: {:?}", entries);
    assert!(entries.iter().any(|e| e.name == "test_real_vec.cbor"));
}