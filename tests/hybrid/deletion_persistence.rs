// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests for deletion persistence in hybrid index

use tokio;
use vector_db::core::chunk::{Manifest, MANIFEST_VERSION};
use vector_db::core::storage::MockS5Storage;
use vector_db::core::types::VectorId;
use vector_db::hybrid::core::HybridIndex;
use vector_db::hybrid::persistence::HybridPersister;
use vector_db::hybrid::HybridConfig;

// Helper function to create a test index with some vectors
async fn create_test_index() -> HybridIndex {
    let config = HybridConfig::default();

    let mut index = HybridIndex::new(config);

    // Initialize index with training data
    let training_data: Vec<Vec<f32>> = (0..10)
        .map(|i| {
            (0..128)
                .map(|j| ((i + j) as f32).sin() * 0.5)
                .collect()
        })
        .collect();

    index.initialize(training_data).await.unwrap();

    index
}

// Helper function to add test vectors
async fn add_test_vectors(index: &mut HybridIndex, count: usize) {
    for i in 0..count {
        let id = VectorId::from_string(&format!("vec-{}", i));
        let vector: Vec<f32> = (0..128).map(|j| ((i + j) as f32).sin() * 0.5).collect();
        index.insert(id, vector).await.unwrap();
    }
}

#[tokio::test]
async fn test_save_index_with_deleted_vectors() {
    // Create index and add vectors
    let mut index = create_test_index().await;
    add_test_vectors(&mut index, 20).await;

    // Delete some vectors
    let vec_5 = VectorId::from_string("vec-5");
    let vec_10 = VectorId::from_string("vec-10");
    let vec_15 = VectorId::from_string("vec-15");

    index.delete(vec_5.clone()).await.unwrap();
    index.delete(vec_10.clone()).await.unwrap();
    index.delete(vec_15.clone()).await.unwrap();

    // Save index
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());
    let manifest = persister
        .save_index_chunked(&index, "test-deleted")
        .await
        .unwrap();

    // Verify manifest includes deleted vectors
    assert_eq!(manifest.version, 3, "Manifest version should be 3");
    assert!(
        manifest.deleted_vectors.is_some(),
        "Manifest should have deleted_vectors field"
    );

    let deleted_vec = manifest.deleted_vectors.unwrap();
    assert_eq!(deleted_vec.len(), 3, "Should have 3 deleted vectors");

    // Convert to set for easier checking
    let deleted_set: std::collections::HashSet<_> = deleted_vec.into_iter().collect();

    // Check using VectorId string representations (which are hashes)
    assert!(deleted_set.contains(&vec_5.to_string()));
    assert!(deleted_set.contains(&vec_10.to_string()));
    assert!(deleted_set.contains(&vec_15.to_string()));
}

#[tokio::test]
async fn test_load_index_with_deleted_vectors() {
    // Create, populate, delete, and save index
    let mut index = create_test_index().await;
    add_test_vectors(&mut index, 20).await;

    let vec_3 = VectorId::from_string("vec-3");
    let vec_7 = VectorId::from_string("vec-7");
    index.delete(vec_3.clone()).await.unwrap();
    index.delete(vec_7.clone()).await.unwrap();

    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());
    persister
        .save_index_chunked(&index, "test-load-deleted")
        .await
        .unwrap();

    // Load index
    let config = HybridConfig::default();

    let loaded_index = persister
        .load_index_chunked("test-load-deleted", config)
        .await
        .unwrap();

    // Verify deleted vectors are marked as deleted
    assert!(
        loaded_index.is_deleted(&vec_3).await,
        "vec-3 should be marked as deleted"
    );
    assert!(
        loaded_index.is_deleted(&vec_7).await,
        "vec-7 should be marked as deleted"
    );

    // Verify non-deleted vectors are not marked as deleted
    let vec_0 = VectorId::from_string("vec-0");
    let vec_5 = VectorId::from_string("vec-5");
    assert!(
        !loaded_index.is_deleted(&vec_0).await,
        "vec-0 should not be deleted"
    );
    assert!(
        !loaded_index.is_deleted(&vec_5).await,
        "vec-5 should not be deleted"
    );
}

#[tokio::test]
async fn test_deleted_vectors_excluded_from_search() {
    // Create, populate, delete, save, and load index
    let mut index = create_test_index().await;
    add_test_vectors(&mut index, 20).await;

    // Delete vectors
    let vec_2 = VectorId::from_string("vec-2");
    let vec_8 = VectorId::from_string("vec-8");
    index.delete(vec_2.clone()).await.unwrap();
    index.delete(vec_8.clone()).await.unwrap();

    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());
    persister
        .save_index_chunked(&index, "test-search-deleted")
        .await
        .unwrap();

    // Load index
    let config = HybridConfig::default();

    let loaded_index = persister
        .load_index_chunked("test-search-deleted", config)
        .await
        .unwrap();

    // Search with vector similar to vec-2
    let query: Vec<f32> = (0..128).map(|j| ((2 + j) as f32).sin() * 0.5).collect();
    let results = loaded_index.search(&query, 10).await.unwrap();

    // Verify deleted vectors not in results
    let result_ids: Vec<String> = results
        .iter()
        .map(|r| r.vector_id.to_string())
        .collect();

    assert!(
        !result_ids.contains(&vec_2.to_string()),
        "Deleted vec-2 should not appear in search results"
    );
    assert!(
        !result_ids.contains(&vec_8.to_string()),
        "Deleted vec-8 should not appear in search results"
    );
}

#[tokio::test]
async fn test_manifest_v3_format() {
    // Create manifest with deleted vectors
    let mut manifest = Manifest::new(10000, 20);
    manifest.deleted_vectors = Some(vec!["vec-1".to_string(), "vec-5".to_string()]);

    // Serialize to JSON
    let json = manifest.to_json().unwrap();

    // Verify JSON contains deleted_vectors field
    assert!(json.contains("deleted_vectors"));
    assert!(json.contains("vec-1"));
    assert!(json.contains("vec-5"));

    // Deserialize back
    let loaded_manifest = Manifest::from_json(&json).unwrap();
    assert_eq!(loaded_manifest.version, 3);
    assert!(loaded_manifest.deleted_vectors.is_some());

    let deleted = loaded_manifest.deleted_vectors.unwrap();
    assert_eq!(deleted.len(), 2);
    assert!(deleted.contains(&"vec-1".to_string()));
    assert!(deleted.contains(&"vec-5".to_string()));
}

#[tokio::test]
async fn test_backward_compatibility_v2_manifest() {
    // Create a v2 manifest JSON (without deleted_vectors field)
    let v2_json = r#"{
        "version": 2,
        "chunk_size": 10000,
        "total_vectors": 10,
        "chunks": [],
        "hnsw_structure": null,
        "ivf_structure": null
    }"#;

    // Should load successfully
    let manifest = Manifest::from_json(v2_json).unwrap();
    assert_eq!(manifest.version, 2);
    assert!(
        manifest.deleted_vectors.is_none(),
        "v2 manifest should not have deleted_vectors"
    );
}

#[tokio::test]
async fn test_forward_compatibility_reject_future_versions() {
    // Create a future version manifest
    let future_json = format!(
        r#"{{
        "version": {},
        "chunk_size": 10000,
        "total_vectors": 10,
        "chunks": [],
        "hnsw_structure": null,
        "ivf_structure": null
    }}"#,
        MANIFEST_VERSION + 1
    );

    // Should reject with version error
    let result = Manifest::from_json(&future_json);
    assert!(result.is_err(), "Should reject future manifest version");

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Invalid version"));
}

#[tokio::test]
async fn test_vacuum_before_save_reduces_tombstones() {
    // Create index and add vectors
    let mut index = create_test_index().await;
    add_test_vectors(&mut index, 30).await;

    // Delete many vectors
    for i in [5, 10, 15, 20, 25] {
        let id = VectorId::from_string(&format!("vec-{}", i));
        index.delete(id).await.unwrap();
    }

    // Vacuum to physically remove deleted vectors
    let vacuum_stats = index.vacuum().await.unwrap();
    assert_eq!(
        vacuum_stats.total_removed, 5,
        "Should remove 5 deleted vectors"
    );

    // Save index
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());
    let manifest = persister
        .save_index_chunked(&index, "test-vacuum")
        .await
        .unwrap();

    // After vacuum, deleted_vectors should be empty or None
    let deleted_count = manifest
        .deleted_vectors
        .as_ref()
        .map(|v| v.len())
        .unwrap_or(0);
    assert_eq!(
        deleted_count, 0,
        "After vacuum, there should be no deleted vectors in manifest"
    );
}

#[tokio::test]
async fn test_active_count_after_load() {
    // Create index with 20 vectors, delete 3
    let mut index = create_test_index().await;
    add_test_vectors(&mut index, 20).await;

    index
        .delete(VectorId::from_string("vec-4"))
        .await
        .unwrap();
    index
        .delete(VectorId::from_string("vec-9"))
        .await
        .unwrap();
    index
        .delete(VectorId::from_string("vec-14"))
        .await
        .unwrap();

    // Save and load
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());
    persister
        .save_index_chunked(&index, "test-active-count")
        .await
        .unwrap();

    let config = HybridConfig::default();

    let loaded_index = persister
        .load_index_chunked("test-active-count", config)
        .await
        .unwrap();

    // Verify active count
    let active_count = loaded_index.active_count().await;
    assert_eq!(active_count, 17, "Should have 17 active vectors (20 - 3)");
}
