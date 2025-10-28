/// Integration tests for large dataset performance
/// Tests chunked storage with 100K, 500K, and 1M vectors
use std::collections::HashMap;
use std::time::Instant;
use chrono::Utc;
use vector_db::core::storage::MockS5Storage;
use vector_db::core::types::VectorId;
use vector_db::hybrid::{HybridConfig, HybridIndex, HybridPersister};
use vector_db::hnsw::core::{HNSWConfig, HNSWIndex, HNSWNode};
use vector_db::ivf::core::{Centroid, ClusterId, IVFConfig, IVFIndex, InvertedList};

// ============================================================================
// Constants
// ============================================================================

const DIMENSIONS: usize = 384; // Standard embedding dimension (all-MiniLM-L6-v2)
const DEFAULT_CHUNK_SIZE: usize = 10_000;

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate test vectors with deterministic values
fn create_test_vectors(count: usize, dimensions: usize, seed: usize) -> Vec<(VectorId, Vec<f32>)> {
    (0..count)
        .map(|i| {
            let id = VectorId::from_string(&format!("vec-{}-{}", seed, i));
            // Create diverse vectors with some structure
            let base = ((i + seed) as f32 * 0.001) % 1.0;
            let vector: Vec<f32> = (0..dimensions)
                .map(|d| base + (d as f32 * 0.0001))
                .collect();
            (id, vector)
        })
        .collect()
}

/// Fast setup that bypasses expensive HNSW construction
/// Creates pre-populated indices directly for testing
async fn setup_large_index(
    vector_count: usize,
    dimensions: usize,
    _chunk_size: usize,
) -> (HybridIndex, Vec<VectorId>, Vec<Vec<f32>>) {
    println!("Setting up index with {} vectors...", vector_count);
    let start = Instant::now();

    // Use default config (chunk_size is handled by persister, not config)
    let config = HybridConfig::default();

    // Create HNSW index
    let mut hnsw_index = HNSWIndex::new(config.hnsw_config.clone());

    // Create IVF index with centroids
    let mut ivf_index = IVFIndex::new(config.ivf_config.clone());
    let num_centroids = config.ivf_config.n_clusters.min(256);
    let centroids: Vec<Centroid> = (0..num_centroids)
        .map(|i| {
            let base = (i as f32 * 0.1) % 1.0;
            let vector = vec![base; dimensions];
            Centroid::new(ClusterId(i), vector)
        })
        .collect();
    ivf_index.set_trained(centroids, dimensions);

    // Generate all vectors
    let test_vectors = create_test_vectors(vector_count, dimensions, 12345);
    let mut ids = Vec::new();
    let mut vectors = Vec::new();
    let mut timestamps = HashMap::new();

    // Split: 30% HNSW (recent), 70% IVF (historical)
    let hnsw_count = (vector_count * 3 / 10).max(100);
    let ivf_count = vector_count - hnsw_count;

    println!(
        "  HNSW vectors: {}, IVF vectors: {}",
        hnsw_count, ivf_count
    );

    // Add vectors to HNSW (recent data)
    let mut first_hnsw_id: Option<VectorId> = None;
    for (id, vector) in test_vectors.iter().take(hnsw_count) {
        let node = HNSWNode::new(id.clone(), vector.clone());
        hnsw_index.restore_node(node).expect("Failed to restore HNSW node");

        // Set first node as entry point
        if first_hnsw_id.is_none() {
            first_hnsw_id = Some(id.clone());
        }

        ids.push(id.clone());
        vectors.push(vector.clone());
        timestamps.insert(id.clone(), Utc::now());
    }

    // Set entry point for HNSW index
    if let Some(entry_id) = first_hnsw_id {
        hnsw_index.set_entry_point(entry_id);
    }

    // Add vectors to IVF (historical data)
    let mut inverted_lists: HashMap<ClusterId, InvertedList> = HashMap::new();
    for i in 0..num_centroids {
        inverted_lists.insert(ClusterId(i), InvertedList::new());
    }

    for (idx, (id, vector)) in test_vectors
        .iter()
        .skip(hnsw_count)
        .take(ivf_count)
        .enumerate()
    {
        // Distribute across clusters for realistic scenario
        let cluster_id = ClusterId(idx % num_centroids);
        let list = inverted_lists.get_mut(&cluster_id).unwrap();
        list.insert(id.clone(), vector.clone())
            .expect("Failed to insert to IVF");
        ids.push(id.clone());
        vectors.push(vector.clone());
        timestamps.insert(id.clone(), Utc::now());
    }

    ivf_index.set_inverted_lists(inverted_lists);

    // Construct HybridIndex from parts
    let index = HybridIndex::from_parts(
        config,
        hnsw_index,
        ivf_index,
        timestamps,
        hnsw_count,
        ivf_count,
    )
    .expect("Failed to create hybrid index");

    println!("  Setup completed in {:.2?}", start.elapsed());
    (index, ids, vectors)
}

// ============================================================================
// Test: 100K Vectors
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --release --test integration_chunked_tests -- --ignored
async fn test_100k_vectors_save_load_search() {
    println!("\n=== Test: 100K Vectors ===");

    let vector_count = 100_000;
    let dimensions = DIMENSIONS;
    let chunk_size = DEFAULT_CHUNK_SIZE;

    // Setup
    let (index, _vector_ids, test_vectors) = setup_large_index(vector_count, dimensions, chunk_size).await;
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Test 1: Save
    println!("\n[1] Saving index...");
    let save_start = Instant::now();
    let manifest = persister
        .save_index_chunked(&index, "test-100k")
        .await
        .expect("Failed to save index");
    let save_time = save_start.elapsed();
    println!("  ✓ Save completed in {:.2?}", save_time);

    // Verify chunk count (100K / 10K = 10 chunks)
    let expected_chunks = (vector_count + chunk_size - 1) / chunk_size;
    println!("  ✓ Chunk count: {} (expected: {})", manifest.chunks.len(), expected_chunks);
    assert_eq!(manifest.chunks.len(), expected_chunks);

    // Test 2: Load
    println!("\n[2] Loading index...");
    let load_start = Instant::now();
    let loaded_index = persister
        .load_index_chunked("test-100k")
        .await
        .expect("Failed to load index");
    let load_time = load_start.elapsed();
    println!("  ✓ Load completed in {:.2?}", load_time);

    // Verify load time target: <5 seconds
    assert!(
        load_time.as_secs() < 5,
        "Load time {:.2?} exceeds 5 second target",
        load_time
    );

    // Verify vector count
    let stats = loaded_index.get_stats();
    println!("  ✓ Vector count: {} (expected: {})", stats.total_vectors, vector_count);
    assert_eq!(stats.total_vectors, vector_count);

    // Test 3: Search correctness
    println!("\n[3] Testing search correctness...");

    // Use first test vector as query
    let query_vector = &test_vectors[0];

    let search_start = Instant::now();
    let results = loaded_index
        .search(query_vector, 10)
        .await
        .expect("Search failed");
    let search_time = search_start.elapsed();

    println!("  ✓ Search completed in {:.2?} (returned {} results)", search_time, results.len());
    assert!(!results.is_empty(), "Search returned no results");

    // Verify top result is the query itself (or very close)
    let top_result = &results[0];
    println!("  ✓ Top result distance: {:.6}", top_result.distance);
    assert!(
        top_result.distance < 0.01,
        "Top result distance too high: {}",
        top_result.distance
    );

    // Test multiple searches for latency
    println!("\n[4] Testing search latency (10 queries)...");
    let mut total_search_time = std::time::Duration::ZERO;
    for i in 0..10 {
        let query_idx = i * 1000; // Spread across dataset
        if query_idx >= test_vectors.len() {
            break;
        }
        let qvec = &test_vectors[query_idx];

        let start = Instant::now();
        let _ = loaded_index.search(qvec, 10).await.expect("Search failed");
        total_search_time += start.elapsed();
    }
    let avg_search_time = total_search_time / 10;
    println!("  ✓ Average search latency: {:.2?}", avg_search_time);

    println!("\n=== 100K Vectors Test PASSED ===\n");
}

// ============================================================================
// Test: 500K Vectors
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --release --test integration_chunked_tests -- --ignored
async fn test_500k_vectors_save_load_search() {
    println!("\n=== Test: 500K Vectors ===");

    let vector_count = 500_000;
    let dimensions = DIMENSIONS;
    let chunk_size = DEFAULT_CHUNK_SIZE;

    // Setup
    let (index, _vector_ids, test_vectors) = setup_large_index(vector_count, dimensions, chunk_size).await;
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Test 1: Save
    println!("\n[1] Saving index...");
    let save_start = Instant::now();
    let manifest = persister
        .save_index_chunked(&index, "test-500k")
        .await
        .expect("Failed to save index");
    let save_time = save_start.elapsed();
    println!("  ✓ Save completed in {:.2?}", save_time);

    // Verify chunk count (500K / 10K = 50 chunks)
    let expected_chunks = (vector_count + chunk_size - 1) / chunk_size;
    println!("  ✓ Chunk count: {} (expected: 50)", manifest.chunks.len());
    assert_eq!(manifest.chunks.len(), expected_chunks);
    assert_eq!(manifest.chunks.len(), 50);

    // Test 2: Load
    println!("\n[2] Loading index...");
    let load_start = Instant::now();
    let loaded_index = persister
        .load_index_chunked("test-500k")
        .await
        .expect("Failed to load index");
    let load_time = load_start.elapsed();
    println!("  ✓ Load completed in {:.2?}", load_time);

    // Verify load time target: <10 seconds
    assert!(
        load_time.as_secs() < 10,
        "Load time {:.2?} exceeds 10 second target",
        load_time
    );

    // Verify vector count
    let stats = loaded_index.get_stats();
    println!("  ✓ Vector count: {} (expected: {})", stats.total_vectors, vector_count);
    assert_eq!(stats.total_vectors, vector_count);

    // Test 3: Search latency
    println!("\n[3] Testing search latency (20 queries)...");
    let mut total_search_time = std::time::Duration::ZERO;
    let num_queries = 20;

    for i in 0..num_queries {
        let query_idx = i * 5000; // Spread across dataset
        if query_idx >= test_vectors.len() {
            break;
        }
        let qvec = &test_vectors[query_idx];

        let start = Instant::now();
        let results = loaded_index.search(qvec, 10).await.expect("Search failed");
        let elapsed = start.elapsed();
        total_search_time += elapsed;

        assert!(!results.is_empty(), "Search returned no results");
    }

    let avg_search_time = total_search_time / num_queries as u32;
    println!("  ✓ Average search latency: {:.2?}", avg_search_time);

    // Target: <100ms per search
    assert!(
        avg_search_time.as_millis() < 100,
        "Average search latency {:.2?} exceeds 100ms target",
        avg_search_time
    );

    println!("\n=== 500K Vectors Test PASSED ===\n");
}

// ============================================================================
// Test: 1M Vectors
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --release --test integration_chunked_tests -- --ignored
async fn test_1m_vectors_save_load_search() {
    println!("\n=== Test: 1M Vectors ===");

    let vector_count = 1_000_000;
    let dimensions = DIMENSIONS;
    let chunk_size = DEFAULT_CHUNK_SIZE;

    // Setup (this will take a while)
    let (index, _vector_ids, test_vectors) = setup_large_index(vector_count, dimensions, chunk_size).await;
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Test 1: Save
    println!("\n[1] Saving index...");
    let save_start = Instant::now();
    let manifest = persister
        .save_index_chunked(&index, "test-1m")
        .await
        .expect("Failed to save index");
    let save_time = save_start.elapsed();
    println!("  ✓ Save completed in {:.2?}", save_time);

    // Verify chunk count (1M / 10K = 100 chunks)
    let expected_chunks = (vector_count + chunk_size - 1) / chunk_size;
    println!("  ✓ Chunk count: {} (expected: 100)", manifest.chunks.len());
    assert_eq!(manifest.chunks.len(), expected_chunks);
    assert_eq!(manifest.chunks.len(), 100);

    // Test 2: Load
    println!("\n[2] Loading index...");
    let load_start = Instant::now();
    let loaded_index = persister
        .load_index_chunked("test-1m")
        .await
        .expect("Failed to load index");
    let load_time = load_start.elapsed();
    println!("  ✓ Load completed in {:.2?}", load_time);

    // Verify load time target: <15 seconds
    assert!(
        load_time.as_secs() < 15,
        "Load time {:.2?} exceeds 15 second target",
        load_time
    );

    // Verify vector count
    let stats = loaded_index.get_stats();
    println!("  ✓ Vector count: {} (expected: {})", stats.total_vectors, vector_count);
    assert_eq!(stats.total_vectors, vector_count);

    // Test 3: Search latency
    println!("\n[3] Testing search latency (30 queries)...");
    let mut total_search_time = std::time::Duration::ZERO;
    let num_queries = 30;

    for i in 0..num_queries {
        let query_idx = i * 10000; // Spread across dataset
        if query_idx >= test_vectors.len() {
            break;
        }
        let qvec = &test_vectors[query_idx];

        let start = Instant::now();
        let results = loaded_index.search(qvec, 10).await.expect("Search failed");
        let elapsed = start.elapsed();
        total_search_time += elapsed;

        assert!(!results.is_empty(), "Search returned no results");
    }

    let avg_search_time = total_search_time / num_queries as u32;
    println!("  ✓ Average search latency: {:.2?}", avg_search_time);

    // Target: <150ms per search
    assert!(
        avg_search_time.as_millis() < 150,
        "Average search latency {:.2?} exceeds 150ms target",
        avg_search_time
    );

    println!("\n=== 1M Vectors Test PASSED ===\n");
}
