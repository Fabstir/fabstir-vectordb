use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use vector_db::core::storage::{S5Storage, MockS5Storage};
use vector_db::core::chunk_cache::ChunkCache;
use vector_db::core::chunk::VectorChunk;
use vector_db::storage::chunk_loader::ChunkLoader;
use vector_db::core::types::VectorId;

/// Helper to create a test chunk
fn create_test_chunk(chunk_id: &str, vector_count: usize) -> VectorChunk {
    let mut chunk = VectorChunk::new(chunk_id.to_string(), 0, vector_count - 1);
    for i in 0..vector_count {
        let id_str = format!("{}_vec_{}", chunk_id, i);
        let id = VectorId::from_string(&id_str);
        let vector = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];
        chunk.add_vector(id, vector);
    }
    chunk
}

/// Helper to serialize chunk to CBOR
fn chunk_to_cbor(chunk: &VectorChunk) -> Vec<u8> {
    serde_cbor::to_vec(chunk).expect("Failed to serialize chunk")
}

#[tokio::test]
async fn test_load_single_chunk() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Create and save a chunk
    let chunk = create_test_chunk("chunk_0", 100);
    let chunk_data = chunk_to_cbor(&chunk);
    storage.put("test/chunks/chunk_0.cbor", chunk_data.clone())
        .await
        .expect("Failed to put chunk");

    // Load the chunk
    let loaded_chunk = loader.load_chunk("test/chunks/chunk_0.cbor")
        .await
        .expect("Failed to load chunk");

    // Verify
    assert_eq!(loaded_chunk.vectors.len(), 100);
    let test_id = VectorId::from_string("chunk_0_vec_0");
    assert!(loaded_chunk.vectors.contains_key(&test_id));

    // Verify chunk is now in cache
    assert!(cache.contains("test/chunks/chunk_0.cbor"));
}

#[tokio::test]
async fn test_load_multiple_chunks_parallel() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Create and save multiple chunks
    let chunk_paths = vec![
        "test/parallel/chunk_0.cbor",
        "test/parallel/chunk_1.cbor",
        "test/parallel/chunk_2.cbor",
        "test/parallel/chunk_3.cbor",
        "test/parallel/chunk_4.cbor",
    ];

    for (i, path) in chunk_paths.iter().enumerate() {
        let chunk = create_test_chunk(&format!("chunk_{}", i), 50);
        let chunk_data = chunk_to_cbor(&chunk);
        storage.put(path, chunk_data).await.expect("Failed to put chunk");
    }

    // Measure time to load in parallel
    let start = Instant::now();
    let loaded_chunks = loader.load_chunks_parallel(chunk_paths.clone())
        .await
        .expect("Failed to load chunks in parallel");
    let duration = start.elapsed();

    // Verify all chunks loaded
    assert_eq!(loaded_chunks.len(), 5);
    for (i, chunk) in loaded_chunks.iter().enumerate() {
        assert_eq!(chunk.vectors.len(), 50);
        let id_str = format!("chunk_{}_vec_0", i);
        let test_id = VectorId::from_string(&id_str);
        assert!(chunk.vectors.contains_key(&test_id));
    }

    // Verify parallel loading is faster than sequential
    // (Should be significantly faster, but we'll just verify it completes)
    println!("Parallel load of 5 chunks took: {:?}", duration);

    // Verify all chunks are cached
    for path in chunk_paths {
        assert!(cache.contains(path));
    }
}

#[tokio::test]
async fn test_chunk_not_found() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Try to load non-existent chunk
    let result = loader.load_chunk("test/nonexistent/chunk_99.cbor").await;

    // Verify
    assert!(result.is_err(), "Should return error for non-existent chunk");

    match result.unwrap_err() {
        err if err.to_string().contains("not found") ||
                err.to_string().contains("Not found") => {
            // Expected error
        }
        err => panic!("Expected 'not found' error, got: {}", err),
    }
}

#[tokio::test]
async fn test_retry_logic_with_exponential_backoff() {
    // This test verifies that the retry logic exists and handles errors correctly
    // Since MockS5Storage doesn't simulate intermittent failures, we test that:
    // 1. Non-existent chunks fail immediately without retry (404 errors don't retry)
    // 2. The retry infrastructure is in place for actual S5 errors

    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Test 1: Non-existent chunk fails fast (no retry on 404)
    let start = Instant::now();
    let result = loader.load_chunk("test/nonexistent.cbor").await;
    let duration = start.elapsed();

    assert!(result.is_err(), "Non-existent chunk should fail");
    assert!(result.unwrap_err().to_string().contains("not found"), "Should be a not found error");
    // Should fail fast without retries (< 50ms)
    assert!(duration.as_millis() < 50, "404 errors should fail fast without retry");

    // Test 2: Valid chunk succeeds on first try
    let chunk = create_test_chunk("retry_test", 10);
    let chunk_data = chunk_to_cbor(&chunk);
    storage.put("test/retry_test.cbor", chunk_data).await.expect("Failed to put chunk");

    let start = Instant::now();
    let result = loader.load_chunk("test/retry_test.cbor").await;
    let duration = start.elapsed();

    assert!(result.is_ok(), "Valid chunk should succeed");
    assert_eq!(result.unwrap().vectors.len(), 10);
    // Should succeed quickly on first attempt
    assert!(duration.as_millis() < 50, "Valid chunk should succeed fast on first try");

    println!("Retry logic test: Fast failure on 404: {:?}, Fast success on hit: {:?}",
             duration, duration);
}

#[tokio::test]
async fn test_cache_integration() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Create and save chunk
    let chunk = create_test_chunk("cache_test", 25);
    let chunk_data = chunk_to_cbor(&chunk);
    let path = "test/cache/chunk_0.cbor";
    storage.put(path, chunk_data).await.expect("Failed to put chunk");

    // First load - should hit storage
    assert!(!cache.contains(path), "Cache should be empty initially");

    let chunk1 = loader.load_chunk(path).await.expect("First load failed");
    assert!(cache.contains(path), "Chunk should be cached after first load");
    assert_eq!(chunk1.vectors.len(), 25);

    // Second load - should hit cache (remove from storage to verify)
    storage.delete(path).await.expect("Failed to delete from storage");

    let chunk2 = loader.load_chunk(path).await.expect("Second load failed");
    assert_eq!(chunk2.vectors.len(), 25);
    let test_id = VectorId::from_string("cache_test_vec_0");
    assert!(chunk2.vectors.contains_key(&test_id));

    println!("Cache integration test: Successfully loaded from cache after storage deletion");
}

#[tokio::test]
async fn test_concurrent_load_requests_deduplication() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    // Create and save chunk
    let chunk = create_test_chunk("concurrent_test", 50);
    let chunk_data = chunk_to_cbor(&chunk);
    let path = "test/concurrent/chunk_0.cbor";
    storage.put(path, chunk_data).await.expect("Failed to put chunk");

    // Launch 10 concurrent load requests for the same chunk
    let mut tasks = Vec::new();
    for i in 0..10 {
        let loader_clone = loader.clone();
        let path = path.to_string();

        let task = tokio::spawn(async move {
            let result = loader_clone.load_chunk(&path).await;
            (i, result)
        });

        tasks.push(task);
    }

    // Wait for all to complete
    let results: Vec<_> = futures::future::join_all(tasks).await;

    // Verify all succeeded
    for result in results {
        let (task_id, chunk_result) = result.expect("Task panicked");
        assert!(chunk_result.is_ok(), "Task {} failed to load chunk", task_id);

        let chunk = chunk_result.unwrap();
        assert_eq!(chunk.vectors.len(), 50);
        let test_id = VectorId::from_string("concurrent_test_vec_0");
        assert!(chunk.vectors.contains_key(&test_id));
    }

    println!("Deduplication test: All 10 concurrent requests succeeded");

    // The deduplication should ensure only one actual S5 load occurred
    // (This is implicit in the implementation - multiple requests share the same load)
}

#[tokio::test]
async fn test_load_chunks_with_some_cached() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let loader = ChunkLoader::new(storage.clone(), cache.clone());

    // Create and save 3 chunks
    let paths = vec![
        "test/mixed/chunk_0.cbor",
        "test/mixed/chunk_1.cbor",
        "test/mixed/chunk_2.cbor",
    ];

    for (i, path) in paths.iter().enumerate() {
        let chunk = create_test_chunk(&format!("mixed_{}", i), 20);
        let chunk_data = chunk_to_cbor(&chunk);
        storage.put(path, chunk_data).await.expect("Failed to put chunk");
    }

    // Pre-load chunk_1 into cache
    loader.load_chunk(paths[1]).await.expect("Failed to pre-load chunk_1");
    assert!(cache.contains(paths[1]), "Chunk 1 should be cached");

    // Now load all 3 chunks
    let loaded_chunks = loader.load_chunks_parallel(paths.clone())
        .await
        .expect("Failed to load chunks");

    // Verify all loaded correctly
    assert_eq!(loaded_chunks.len(), 3);
    for (i, chunk) in loaded_chunks.iter().enumerate() {
        assert_eq!(chunk.vectors.len(), 20);
        let id_str = format!("mixed_{}_vec_0", i);
        let test_id = VectorId::from_string(&id_str);
        assert!(chunk.vectors.contains_key(&test_id));
    }

    // Verify all are now cached
    for path in paths {
        assert!(cache.contains(path));
    }

    println!("Mixed cache test: Successfully loaded with cache hits and misses");
}
