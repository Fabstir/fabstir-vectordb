// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// Integration tests for chunked load operations
use vector_db::core::storage::{MockS5Storage, S5Storage};
use vector_db::core::types::VectorId;
use vector_db::hybrid::{HybridConfig, HybridIndex, HybridPersister};
use vector_db::hnsw::core::{HNSWIndex, HNSWNode};
use vector_db::ivf::core::{Centroid, ClusterId, IVFIndex, InvertedList};
use chrono::Utc;
use std::collections::HashMap;

// ============================================================================
// Helper Functions (reuse from chunked_save_tests.rs)
// ============================================================================

fn create_test_vectors(count: usize, dimensions: usize) -> Vec<(VectorId, Vec<f32>)> {
    (0..count)
        .map(|i| {
            let id = VectorId::from_string(&format!("vec{}", i));
            let vector = vec![i as f32 * 0.01; dimensions];
            (id, vector)
        })
        .collect()
}

/// Fast test helper that bypasses expensive HNSW construction
async fn setup_index_with_vectors_fast(vector_count: usize) -> (HybridIndex, Vec<VectorId>) {
    let config = HybridConfig::default();
    let dimensions = 4;

    let mut hnsw_index = HNSWIndex::new(config.hnsw_config.clone());
    let mut ivf_index = IVFIndex::new(config.ivf_config.clone());

    let num_centroids = config.ivf_config.n_clusters.min(10);
    let centroids: Vec<Centroid> = (0..num_centroids)
        .map(|i| {
            let vector = vec![i as f32 * 0.1; dimensions];
            Centroid::new(ClusterId(i), vector)
        })
        .collect();

    ivf_index.set_trained(centroids, dimensions);

    let test_vectors = create_test_vectors(vector_count, dimensions);
    let mut ids = Vec::new();
    let mut timestamps = HashMap::new();

    let hnsw_count = (vector_count / 2).max(1);
    let ivf_count = vector_count - hnsw_count;

    for (_i, (id, vector)) in test_vectors.iter().take(hnsw_count).enumerate() {
        let node = HNSWNode::new(id.clone(), vector.clone());
        hnsw_index.restore_node(node).expect("Failed to restore node");
        ids.push(id.clone());
        timestamps.insert(id.clone(), Utc::now());
    }

    let mut inverted_lists: HashMap<ClusterId, InvertedList> = HashMap::new();
    for i in 0..num_centroids {
        inverted_lists.insert(ClusterId(i), InvertedList::new());
    }

    for (i, (id, vector)) in test_vectors.iter().skip(hnsw_count).take(ivf_count).enumerate() {
        let cluster_id = ClusterId(i % num_centroids);
        let list = inverted_lists.get_mut(&cluster_id).unwrap();
        list.insert(id.clone(), vector.clone()).expect("Failed to insert to IVF list");
        ids.push(id.clone());
        timestamps.insert(id.clone(), Utc::now());
    }

    ivf_index.set_inverted_lists(inverted_lists);

    let index = HybridIndex::from_parts(
        config,
        hnsw_index,
        ivf_index,
        timestamps,
        hnsw_count,
        ivf_count,
    ).expect("Failed to create index from parts");

    (index, ids)
}

// ============================================================================
// Empty Index Load Tests
// ============================================================================

#[tokio::test]
async fn test_load_empty_index() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Save empty index
    let config = HybridConfig::default();
    let index = HybridIndex::new(config);

    persister.save_index_chunked(&index, "test/empty").await.expect("Failed to save");

    // Load it back
    let loaded_index = persister.load_index_chunked("test/empty").await.expect("Failed to load");

    // Verify stats match
    let original_stats = index.get_stats();
    let loaded_stats = loaded_index.get_stats();

    assert_eq!(loaded_stats.total_vectors, original_stats.total_vectors);
    assert_eq!(loaded_stats.total_vectors, 0);
}

// ============================================================================
// Single Chunk Load Tests
// ============================================================================

#[tokio::test]
async fn test_load_single_chunk() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Save index with 10 vectors
    let (index, ids) = setup_index_with_vectors_fast(10).await;
    let original_stats = index.get_stats();

    persister.save_index_chunked(&index, "test/single").await.expect("Failed to save");

    // Load it back
    let loaded_index = persister.load_index_chunked("test/single").await.expect("Failed to load");
    let loaded_stats = loaded_index.get_stats();

    // Verify vector count preserved
    assert_eq!(loaded_stats.total_vectors, original_stats.total_vectors);
    assert_eq!(loaded_stats.total_vectors, 10);
}

#[tokio::test]
async fn test_load_and_verify_vector_counts() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(100).await;
    let original_stats = index.get_stats();

    persister.save_index_chunked(&index, "test/counts").await.expect("Failed to save");

    let loaded_index = persister.load_index_chunked("test/counts").await.expect("Failed to load");
    let loaded_stats = loaded_index.get_stats();

    assert_eq!(loaded_stats.total_vectors, original_stats.total_vectors);
    assert_eq!(loaded_stats.recent_vectors, original_stats.recent_vectors);
    assert_eq!(loaded_stats.historical_vectors, original_stats.historical_vectors);
}

// ============================================================================
// Multi-Chunk Load Tests
// ============================================================================

#[tokio::test]
async fn test_load_multi_chunk_index() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Save index with 25K vectors (3 chunks)
    let (index, _ids) = setup_index_with_vectors_fast(25000).await;
    let original_stats = index.get_stats();

    persister.save_index_chunked(&index, "test/25k").await.expect("Failed to save");

    // Load it back
    let loaded_index = persister.load_index_chunked("test/25k").await.expect("Failed to load");
    let loaded_stats = loaded_index.get_stats();

    // Verify all vectors loaded
    assert_eq!(loaded_stats.total_vectors, original_stats.total_vectors);
    assert_eq!(loaded_stats.total_vectors, 25000);
}

// ============================================================================
// Structure Reconstruction Tests
// ============================================================================

#[tokio::test]
async fn test_hnsw_structure_reconstruction() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(100).await;
    let original_stats = index.get_stats();

    persister.save_index_chunked(&index, "test/hnsw").await.expect("Failed to save");

    let loaded_index = persister.load_index_chunked("test/hnsw").await.expect("Failed to load");
    let loaded_stats = loaded_index.get_stats();

    // Verify HNSW vectors reconstructed
    assert_eq!(loaded_stats.recent_vectors, original_stats.recent_vectors);
}

#[tokio::test]
async fn test_ivf_structure_reconstruction() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(100).await;
    let original_stats = index.get_stats();

    persister.save_index_chunked(&index, "test/ivf").await.expect("Failed to save");

    let loaded_index = persister.load_index_chunked("test/ivf").await.expect("Failed to load");
    let loaded_stats = loaded_index.get_stats();

    // Verify IVF vectors reconstructed
    assert_eq!(loaded_stats.historical_vectors, original_stats.historical_vectors);
}

// ============================================================================
// Search Correctness Tests
// ============================================================================

#[tokio::test]
async fn test_search_after_load() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(100).await;

    // Perform search before save
    let query = vec![0.05; 4];
    let original_results = index.search(&query, 5).await.expect("Search failed");

    // Save and load
    persister.save_index_chunked(&index, "test/search").await.expect("Failed to save");
    let loaded_index = persister.load_index_chunked("test/search").await.expect("Failed to load");

    // Perform search after load
    let loaded_results = loaded_index.search(&query, 5).await.expect("Search failed after load");

    // Verify same number of results
    assert_eq!(loaded_results.len(), original_results.len());

    // Note: Exact result ordering may differ due to ties in distances,
    // but result count and approximate quality should match
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_load_missing_manifest() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage);

    // Try to load non-existent index
    let result = persister.load_index_chunked("test/nonexistent").await;

    assert!(result.is_err(), "Should fail when manifest is missing");
}

#[tokio::test]
async fn test_load_corrupted_manifest() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Save corrupted JSON manifest
    let corrupted_json = b"{ this is not valid json }";
    storage.put("test/corrupted/manifest.json", corrupted_json.to_vec())
        .await
        .expect("Failed to save corrupted manifest");

    // Try to load
    let result = persister.load_index_chunked("test/corrupted").await;

    assert!(result.is_err(), "Should fail when manifest is corrupted");
}

#[tokio::test]
async fn test_load_version_mismatch() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Create manifest with future version
    let future_manifest = serde_json::json!({
        "version": 99,
        "chunk_size": 10000,
        "total_vectors": 0,
        "chunks": []
    });

    let manifest_json = serde_json::to_string(&future_manifest).unwrap();
    storage.put("test/future/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save future manifest");

    // Try to load
    let result = persister.load_index_chunked("test/future").await;

    assert!(result.is_err(), "Should fail when version is incompatible");
}
