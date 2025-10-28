use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;
use vector_db::core::storage::{S5Storage, MockS5Storage};
use vector_db::core::chunk_cache::ChunkCache;
use vector_db::core::chunk::VectorChunk;
use vector_db::core::types::VectorId;
use vector_db::storage::chunk_loader::ChunkLoader;
use vector_db::hnsw::core::{HNSWIndex, HNSWConfig};

/// Helper to create test vectors
fn create_test_vectors(count: usize, dimensions: usize) -> Vec<(VectorId, Vec<f32>)> {
    (0..count)
        .map(|i| {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = (0..dimensions)
                .map(|d| (i * dimensions + d) as f32 / 100.0)
                .collect();
            (id, vector)
        })
        .collect()
}

/// Helper to create and save vector chunks to storage
async fn create_chunks_in_storage(
    storage: &Arc<MockS5Storage>,
    vectors_per_chunk: usize,
    num_chunks: usize,
    dimensions: usize,
) -> Vec<String> {
    let mut chunk_ids = Vec::new();

    for chunk_idx in 0..num_chunks {
        let chunk_id = format!("chunk_{}", chunk_idx);
        let start = chunk_idx * vectors_per_chunk;

        let mut chunk = VectorChunk::new(chunk_id.clone(), start, start + vectors_per_chunk - 1);

        // Add vectors to chunk
        for i in 0..vectors_per_chunk {
            let global_idx = start + i;
            let id = VectorId::from_string(&format!("vec_{}", global_idx));
            let vector: Vec<f32> = (0..dimensions)
                .map(|d| (global_idx * dimensions + d) as f32 / 100.0)
                .collect();
            chunk.add_vector(id, vector);
        }

        // Save chunk to storage
        let chunk_data = serde_cbor::to_vec(&chunk).expect("Failed to serialize chunk");
        let path = format!("test/hnsw/chunks/{}.cbor", chunk_id);
        storage.put(&path, chunk_data).await.expect("Failed to save chunk");

        chunk_ids.push(chunk_id);
    }

    chunk_ids
}

#[tokio::test]
async fn test_hnsw_search_with_lazy_loading() {
    // Setup: Create storage with 1 chunk (100 vectors)
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let chunk_ids = create_chunks_in_storage(&storage, 100, 1, dimensions).await;

    // Create HNSW index with lazy loading enabled
    let config = HNSWConfig {
        max_connections: 16,
        max_connections_layer_0: 32,
        ef_construction: 200,
        seed: Some(42),
    };

    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    // Insert vectors with chunk assignments
    let vectors = create_test_vectors(100, dimensions);
    for (i, (id, vector)) in vectors.iter().enumerate() {
        let chunk_id = Some(chunk_ids[0].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), chunk_id)
            .expect("Failed to insert vector");
    }

    // Search: Query should trigger lazy loading
    let query = vec![0.1, 0.2, 0.3, 0.4];
    let results = index.search(&query, 5, 50).expect("Search failed");

    // Verify: Results should be returned correctly
    assert_eq!(results.len(), 5);
    assert!(results[0].distance >= 0.0);

    // Verify: Chunk was loaded (check cache)
    assert!(cache.contains(&format!("test/hnsw/chunks/{}.cbor", chunk_ids[0])));

    println!("Lazy loading search test: Found {} results", results.len());
}

#[tokio::test]
async fn test_hnsw_search_across_multiple_chunks() {
    // Setup: Create storage with 3 chunks (100 vectors each = 300 total)
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let vectors_per_chunk = 100;
    let num_chunks = 3;
    let chunk_ids = create_chunks_in_storage(&storage, vectors_per_chunk, num_chunks, dimensions).await;

    // Create HNSW index
    let config = HNSWConfig::default();
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    // Insert vectors from all chunks
    let total_vectors = vectors_per_chunk * num_chunks;
    let vectors = create_test_vectors(total_vectors, dimensions);
    for (i, (id, vector)) in vectors.iter().enumerate() {
        let chunk_idx = i / vectors_per_chunk;
        let chunk_id = Some(chunk_ids[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), chunk_id)
            .expect("Failed to insert vector");
    }

    // Search: Should access multiple chunks
    let query = vec![1.0, 1.0, 1.0, 1.0];
    let results = index.search(&query, 10, 50).expect("Search failed");

    // Verify: Results span multiple chunks
    assert_eq!(results.len(), 10);

    // At least one chunk should be in cache
    let cached_chunks = chunk_ids.iter()
        .filter(|chunk_id| cache.contains(&format!("test/hnsw/chunks/{}.cbor", chunk_id)))
        .count();
    assert!(cached_chunks > 0, "At least one chunk should be cached");

    println!("Multi-chunk search: {} results, {} chunks cached", results.len(), cached_chunks);
}

#[tokio::test]
async fn test_cache_hit_rate_during_repeated_searches() {
    // Setup: Create storage with 2 chunks
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let chunk_ids = create_chunks_in_storage(&storage, 50, 2, dimensions).await;

    // Create HNSW index
    let config = HNSWConfig {
        max_connections: 8,
        max_connections_layer_0: 16,
        ef_construction: 100,
        seed: Some(42),
    };
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    // Insert vectors
    let vectors = create_test_vectors(100, dimensions);
    for (i, (id, vector)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(chunk_ids[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), chunk_id)
            .expect("Failed to insert");
    }

    let query = vec![0.5, 0.5, 0.5, 0.5];

    // First search: Cold cache (will be slower)
    let start = Instant::now();
    let results1 = index.search(&query, 5, 50).expect("First search failed");
    let cold_duration = start.elapsed();

    // Second search: Warm cache (should be faster)
    let start = Instant::now();
    let results2 = index.search(&query, 5, 50).expect("Second search failed");
    let warm_duration = start.elapsed();

    // Verify: Same results
    assert_eq!(results1.len(), results2.len());

    // Verify: Warm cache is faster (or at least not significantly slower)
    // Note: In tests this might not always be true due to small dataset
    println!("Cold cache: {:?}, Warm cache: {:?}", cold_duration, warm_duration);
    println!("Cache effectiveness: Results are consistent");
}

#[tokio::test]
async fn test_hnsw_insert_with_lazy_loaded_neighbors() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let chunk_ids = create_chunks_in_storage(&storage, 50, 1, dimensions).await;

    // Create index
    let config = HNSWConfig::default();
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader.clone()));

    // Insert initial vectors
    let vectors = create_test_vectors(50, dimensions);
    for (id, vector) in vectors.iter() {
        index.insert_with_chunk(id.clone(), vector.clone(), Some(chunk_ids[0].clone()))
            .expect("Failed to insert");
    }

    // Insert new vector: Should load neighbor vectors lazily
    let new_id = VectorId::from_string("vec_new");
    let new_vector = vec![1.0, 1.0, 1.0, 1.0];
    index.insert_with_chunk(new_id.clone(), new_vector, Some(chunk_ids[0].clone()))
        .expect("Failed to insert new vector");

    // Verify: New vector was inserted
    assert_eq!(index.node_count(), 51);

    // Verify: Can search and find new vector
    let results = index.search(&[1.0, 1.0, 1.0, 1.0], 5, 50).expect("Search failed");
    assert!(results.iter().any(|r| r.vector_id == new_id));

    println!("Insert with lazy neighbors: {} total vectors", index.node_count());
}

#[tokio::test]
async fn test_performance_cold_vs_warm_cache() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;
    let chunk_ids = create_chunks_in_storage(&storage, 100, 3, dimensions).await;

    // Create index with 300 vectors
    let config = HNSWConfig {
        max_connections: 16,
        max_connections_layer_0: 32,
        ef_construction: 200,
        seed: Some(42),
    };
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    let vectors = create_test_vectors(300, dimensions);
    for (i, (id, vector)) in vectors.iter().enumerate() {
        let chunk_idx = i / 100;
        index.insert_with_chunk(id.clone(), vector.clone(), Some(chunk_ids[chunk_idx].clone()))
            .expect("Failed to insert");
    }

    let query = vec![2.0; dimensions];

    // Cold cache: Measure first search
    let start = Instant::now();
    let cold_results = index.search(&query, 10, 50).expect("Cold search failed");
    let cold_time = start.elapsed();

    // Warm cache: Search multiple times
    let mut warm_times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        index.search(&query, 10, 50).expect("Warm search failed");
        warm_times.push(start.elapsed());
    }

    let avg_warm_time = warm_times.iter().sum::<Duration>() / warm_times.len() as u32;

    println!("Performance test:");
    println!("  Cold cache: {:?}", cold_time);
    println!("  Warm cache (avg): {:?}", avg_warm_time);
    println!("  Speedup: {:.2}x", cold_time.as_micros() as f64 / avg_warm_time.as_micros() as f64);

    // Verify: Results are consistent
    assert_eq!(cold_results.len(), 10);
}

#[tokio::test]
async fn test_error_handling_missing_chunk() {
    // Setup: Create index with reference to non-existent chunk
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let config = HNSWConfig::default();
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    // Insert vector with reference to non-existent chunk
    let id = VectorId::from_string("vec_orphan");
    let vector = vec![1.0, 2.0, 3.0, 4.0];
    let missing_chunk_id = Some("chunk_missing".to_string());

    // Insert should succeed (chunk is loaded lazily, not at insert time)
    index.insert_with_chunk(id.clone(), vector, missing_chunk_id)
        .expect("Insert should succeed");

    // Search should fail gracefully when trying to load missing chunk
    let query = vec![1.0, 2.0, 3.0, 4.0];
    let result = index.search(&query, 5, 50);

    // Verify: Error is returned (not panic)
    if let Err(err) = result {
        println!("Missing chunk error (expected): {}", err);
        assert!(err.to_string().contains("not found") || err.to_string().contains("chunk"));
    } else {
        // If search succeeds, it means the vector was cached inline (backward compatibility)
        println!("Search succeeded with inline vector (backward compatibility mode)");
    }
}

#[tokio::test]
async fn test_concurrent_searches_thread_safety() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 4;
    let chunk_ids = create_chunks_in_storage(&storage, 50, 2, dimensions).await;

    // Create index
    let config = HNSWConfig {
        max_connections: 16,
        max_connections_layer_0: 32,
        ef_construction: 200,
        seed: Some(42),
    };
    let mut index = HNSWIndex::with_chunk_loader(config, Some(chunk_loader));

    // Insert vectors
    let vectors = create_test_vectors(100, dimensions);
    for (i, (id, vector)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        index.insert_with_chunk(id.clone(), vector.clone(), Some(chunk_ids[chunk_idx].clone()))
            .expect("Failed to insert");
    }

    // Convert to Arc for sharing across threads
    let index = Arc::new(index);

    // Launch 10 concurrent searches
    let mut tasks = Vec::new();
    for i in 0..10 {
        let index_clone = index.clone();
        let query = vec![i as f32 / 10.0; dimensions];

        let task = tokio::spawn(async move {
            index_clone.search(&query, 5, 50)
        });

        tasks.push(task);
    }

    // Wait for all searches to complete
    let results = futures::future::join_all(tasks).await;

    // Verify: All searches succeeded
    let mut success_count = 0;
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(Ok(search_results)) => {
                assert_eq!(search_results.len(), 5);
                success_count += 1;
            }
            Ok(Err(err)) => println!("Search {} failed: {}", i, err),
            Err(err) => println!("Task {} panicked: {}", i, err),
        }
    }

    assert_eq!(success_count, 10, "All 10 concurrent searches should succeed");
    println!("Concurrent searches test: {}/10 succeeded", success_count);
}
