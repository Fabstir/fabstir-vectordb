/// Integration tests for chunked save operations
use vector_db::core::chunk::{Manifest, MANIFEST_VERSION};
use vector_db::core::storage::{MockS5Storage, S5Storage};
use vector_db::core::types::VectorId;
use vector_db::hybrid::{HybridConfig, HybridIndex, HybridPersister};
use vector_db::hnsw::core::{HNSWIndex, HNSWNode};
use vector_db::ivf::core::{Centroid, ClusterId, IVFIndex, InvertedList};
use chrono::Utc;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
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
/// Creates pre-populated indices directly for testing
async fn setup_index_with_vectors_fast(vector_count: usize) -> (HybridIndex, Vec<VectorId>) {
    let config = HybridConfig::default();
    let dimensions = 4;

    // Create HNSW index with nodes added directly
    let mut hnsw_index = HNSWIndex::new(config.hnsw_config.clone());

    // Create IVF index with centroids and inverted lists
    let mut ivf_index = IVFIndex::new(config.ivf_config.clone());

    // Set up IVF centroids (simple: just use first few vectors as centroids)
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

    // Split vectors between HNSW (recent) and IVF (historical)
    let hnsw_count = (vector_count / 2).max(1);
    let ivf_count = vector_count - hnsw_count;

    // Add vectors to HNSW directly using restore_node
    for (i, (id, vector)) in test_vectors.iter().take(hnsw_count).enumerate() {
        let node = HNSWNode::new(id.clone(), vector.clone());
        hnsw_index.restore_node(node).expect("Failed to restore node");
        ids.push(id.clone());
        timestamps.insert(id.clone(), Utc::now());
    }

    // Add vectors to IVF directly using set_inverted_lists
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

    // Construct HybridIndex from parts
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
// Empty Index Tests
// ============================================================================

#[tokio::test]
async fn test_save_empty_index() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let config = HybridConfig::default();
    let index = HybridIndex::new(config);

    // Save empty index
    let result = persister.save_index_chunked(&index, "test/empty").await;

    assert!(result.is_ok(), "Failed to save empty index: {:?}", result.err());

    // Verify manifest was saved
    let manifest_data = storage
        .get("test/empty/manifest.json")
        .await
        .expect("Failed to get manifest")
        .expect("Manifest not found");

    let manifest_str = String::from_utf8(manifest_data).expect("Invalid UTF-8");
    let manifest: Manifest = serde_json::from_str(&manifest_str).expect("Invalid JSON");

    assert_eq!(manifest.version, MANIFEST_VERSION);
    assert_eq!(manifest.total_vectors, 0);
    assert_eq!(manifest.chunks.len(), 0);
}

// ============================================================================
// Small Dataset Tests (< 10K vectors = 1 chunk)
// ============================================================================

#[tokio::test]
async fn test_save_single_chunk() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Use small dataset for testing (10 vectors)
    let (index, _ids) = setup_index_with_vectors_fast(10).await;

    // Save index
    let result = persister.save_index_chunked(&index, "test/single").await;
    assert!(result.is_ok(), "Failed to save: {:?}", result.err());

    // Verify manifest
    let manifest_data = storage
        .get("test/single/manifest.json")
        .await
        .expect("Failed to get manifest")
        .expect("Manifest not found");

    let manifest_str = String::from_utf8(manifest_data).expect("Invalid UTF-8");
    let manifest: Manifest = serde_json::from_str(&manifest_str).expect("Invalid JSON");

    assert_eq!(manifest.total_vectors, 10);
    assert_eq!(manifest.chunks.len(), 1);
    assert_eq!(manifest.chunks[0].vector_count, 10);

    // Verify chunk was saved
    let chunk_data = storage.get("test/single/chunks/chunk-0.cbor").await.expect("Failed to get chunk");
    assert!(chunk_data.is_some(), "Chunk not saved");
}

#[tokio::test]
async fn test_save_5000_vectors() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(5000).await;

    let result = persister.save_index_chunked(&index, "test/5k").await;
    assert!(result.is_ok());

    // Load manifest
    let manifest_data = storage.get("test/5k/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    assert_eq!(manifest.total_vectors, 5000);
    assert_eq!(manifest.chunks.len(), 1); // All fit in one chunk
}

// ============================================================================
// Multiple Chunk Tests
// ============================================================================
// NOTE: All multi-chunk tests are temporarily ignored due to slow vector insertion.
// TODO: Implement batch insert API or use pre-populated mock indices for these tests.

#[tokio::test]
async fn test_save_25k_vectors_three_chunks() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(25000).await;

    let result = persister.save_index_chunked(&index, "test/25k").await;
    assert!(result.is_ok(), "Failed to save: {:?}", result.err());

    // Load manifest
    let manifest_data = storage.get("test/25k/manifest.json").await.unwrap().unwrap();
    let manifest_str = String::from_utf8(manifest_data).unwrap();
    let manifest: Manifest = serde_json::from_str(&manifest_str).unwrap();

    assert_eq!(manifest.total_vectors, 25000);
    assert_eq!(manifest.chunks.len(), 3); // 10K + 10K + 5K

    // Verify chunk sizes
    assert_eq!(manifest.chunks[0].vector_count, 10000);
    assert_eq!(manifest.chunks[1].vector_count, 10000);
    assert_eq!(manifest.chunks[2].vector_count, 5000);

    // Verify all chunks exist
    for i in 0..3 {
        let chunk_path = format!("test/25k/chunks/chunk-{}.cbor", i);
        let chunk_data = storage.get(&chunk_path).await.expect("Failed to get chunk");
        assert!(chunk_data.is_some(), "Chunk {} not found", i);
    }
}

#[tokio::test]
async fn test_save_exactly_10k_vectors() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(10000).await;

    let result = persister.save_index_chunked(&index, "test/10k").await;
    assert!(result.is_ok());

    let manifest_data = storage.get("test/10k/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    assert_eq!(manifest.total_vectors, 10000);
    assert_eq!(manifest.chunks.len(), 1); // Exactly one chunk
    assert_eq!(manifest.chunks[0].vector_count, 10000);
}

#[tokio::test]
async fn test_save_10001_vectors_two_chunks() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(10001).await;

    let result = persister.save_index_chunked(&index, "test/10k1").await;
    assert!(result.is_ok());

    let manifest_data = storage.get("test/10k1/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    assert_eq!(manifest.total_vectors, 10001);
    assert_eq!(manifest.chunks.len(), 2); // 10K + 1
    assert_eq!(manifest.chunks[0].vector_count, 10000);
    assert_eq!(manifest.chunks[1].vector_count, 1);
}

// ============================================================================
// Chunk Metadata Tests
// ============================================================================

#[tokio::test]
async fn test_chunk_metadata_accuracy() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(15000).await;

    persister.save_index_chunked(&index, "test/meta").await.unwrap();

    let manifest_data = storage.get("test/meta/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    assert_eq!(manifest.chunks.len(), 2);

    // Check chunk IDs
    assert_eq!(manifest.chunks[0].chunk_id, "chunk-0");
    assert_eq!(manifest.chunks[1].chunk_id, "chunk-1");

    // Check vector counts
    assert_eq!(manifest.chunks[0].vector_count, 10000);
    assert_eq!(manifest.chunks[1].vector_count, 5000);

    // Check byte sizes are non-zero
    assert!(manifest.chunks[0].byte_size > 0);
    assert!(manifest.chunks[1].byte_size > 0);

    // Check vector ID ranges exist
    assert!(manifest.chunks[0].vector_id_range.0.to_string().starts_with("vec"));
    assert!(manifest.chunks[0].vector_id_range.1.to_string().starts_with("vec"));
}

// ============================================================================
// Manifest Generation Tests
// ============================================================================

#[tokio::test]
async fn test_manifest_has_version() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(10).await;

    persister.save_index_chunked(&index, "test/version").await.unwrap();

    let manifest_data = storage.get("test/version/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    assert_eq!(manifest.version, MANIFEST_VERSION);
    assert_eq!(manifest.chunk_size, 10000); // Default chunk size
}

#[tokio::test]
async fn test_manifest_chunk_list() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(25000).await;

    persister.save_index_chunked(&index, "test/list").await.unwrap();

    let manifest_data = storage.get("test/list/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    // Verify chunk list
    assert_eq!(manifest.chunks.len(), 3);

    // Verify chunk IDs are sequential
    for (i, chunk) in manifest.chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_id, format!("chunk-{}", i));
    }

    // Verify total vectors match sum of chunk counts
    let total: usize = manifest.chunks.iter().map(|c| c.vector_count).sum();
    assert_eq!(total, 25000);
}

// ============================================================================
// HNSW Structure Preservation Tests
// ============================================================================

#[tokio::test]
async fn test_hnsw_structure_in_manifest() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(20).await;

    persister.save_index_chunked(&index, "test/hnsw").await.unwrap();

    let manifest_data = storage.get("test/hnsw/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    // HNSW structure should be present
    assert!(manifest.hnsw_structure.is_some(), "HNSW structure missing");

    let hnsw = manifest.hnsw_structure.unwrap();

    // Should have layers
    assert!(!hnsw.layers.is_empty(), "No layers in HNSW structure");

    // Should have node-to-chunk mapping
    assert!(!hnsw.node_chunk_map.is_empty(), "No node mappings");
}

// ============================================================================
// IVF Structure Preservation Tests
// ============================================================================

#[tokio::test]
async fn test_ivf_structure_in_manifest() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(20).await;

    persister.save_index_chunked(&index, "test/ivf").await.unwrap();

    let manifest_data = storage.get("test/ivf/manifest.json").await.unwrap().unwrap();
    let manifest: Manifest = serde_json::from_str(&String::from_utf8(manifest_data).unwrap()).unwrap();

    // IVF structure should be present
    assert!(manifest.ivf_structure.is_some(), "IVF structure missing");

    let ivf = manifest.ivf_structure.unwrap();

    // Should have centroids
    assert!(!ivf.centroids.is_empty(), "No centroids in IVF structure");

    // Should have cluster assignments
    assert!(!ivf.cluster_assignments.is_empty(), "No cluster assignments");
}

// ============================================================================
// Metadata File Tests
// ============================================================================

#[tokio::test]
async fn test_metadata_saved() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(10).await;

    persister.save_index_chunked(&index, "test/metadata").await.unwrap();

    // Verify metadata file exists
    let metadata_data = storage.get("test/metadata/metadata.cbor").await.expect("Failed to check metadata");
    assert!(metadata_data.is_some(), "Metadata file not saved");
}

// ============================================================================
// Storage Path Tests
// ============================================================================

#[tokio::test]
async fn test_correct_storage_paths() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    let (index, _ids) = setup_index_with_vectors_fast(15000).await;

    persister.save_index_chunked(&index, "my/custom/path").await.unwrap();

    // Verify paths
    assert!(storage.get("my/custom/path/manifest.json").await.unwrap().is_some());
    assert!(storage.get("my/custom/path/chunks/chunk-0.cbor").await.unwrap().is_some());
    assert!(storage.get("my/custom/path/chunks/chunk-1.cbor").await.unwrap().is_some());
    assert!(storage.get("my/custom/path/metadata.cbor").await.unwrap().is_some());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_save_with_invalid_path() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage);

    let (index, _ids) = setup_index_with_vectors_fast(10).await;

    // Empty path should fail
    let result = persister.save_index_chunked(&index, "").await;
    assert!(result.is_err(), "Should fail with empty path");
}
