// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use vector_db::core::types::VectorId;
use vector_db::ivf::core::{IVFConfig, IVFIndex};

/// Helper function to create a simple trained IVF index for testing
async fn create_test_index() -> IVFIndex {
    let config = IVFConfig {
        n_clusters: 4,
        n_probe: 4,  // Search all clusters for testing
        train_size: 100,
        max_iterations: 10,
        seed: Some(42),
    };

    let mut index = IVFIndex::new(config);

    // Generate training data (384-dim vectors) - use same pattern as insertion
    let training_data: Vec<Vec<f32>> = (0..100)
        .map(|i| {
            (0..384)
                .map(|j| ((i + j) as f32 * 0.01))
                .collect()
        })
        .collect();

    // Train the index
    index.train(&training_data).unwrap();

    // Insert some vectors using similar pattern to training
    for i in 0..20 {
        let id = VectorId::from_string(&format!("vec_{}", i));
        let vector: Vec<f32> = (0..384).map(|j| ((i + j) as f32 * 0.01)).collect();
        index.insert(id, vector).unwrap();
    }

    index
}

#[tokio::test]
async fn test_mark_deleted() {
    let mut index = create_test_index().await;
    let id = VectorId::from_string("vec_5");

    // Initially not deleted
    assert!(!index.is_deleted(&id));

    // Mark as deleted
    let result = index.mark_deleted(&id);
    assert!(result.is_ok());

    // Now should be deleted
    assert!(index.is_deleted(&id));
}

#[tokio::test]
async fn test_is_deleted() {
    let mut index = create_test_index().await;
    let id_existing = VectorId::from_string("vec_3");
    let id_nonexistent = VectorId::from_string("vec_999");

    // Existing vector not deleted
    assert!(!index.is_deleted(&id_existing));

    // Mark as deleted
    index.mark_deleted(&id_existing).unwrap();

    // Now should be deleted
    assert!(index.is_deleted(&id_existing));

    // Non-existent vector should return false (not deleted, just doesn't exist)
    assert!(!index.is_deleted(&id_nonexistent));
}

#[tokio::test]
async fn test_batch_delete() {
    let mut index = create_test_index().await;

    let ids = vec![
        VectorId::from_string("vec_0"),
        VectorId::from_string("vec_1"),
        VectorId::from_string("vec_2"),
        VectorId::from_string("vec_999"), // Non-existent
    ];

    let result = index.batch_delete(&ids).unwrap();

    // Should successfully delete 3, fail 1
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 1);
    assert_eq!(result.errors.len(), 1);

    // Verify deleted vectors are marked
    assert!(index.is_deleted(&VectorId::from_string("vec_0")));
    assert!(index.is_deleted(&VectorId::from_string("vec_1")));
    assert!(index.is_deleted(&VectorId::from_string("vec_2")));
    assert!(!index.is_deleted(&VectorId::from_string("vec_3"))); // Not deleted
}

#[tokio::test]
async fn test_search_excludes_deleted() {
    let mut index = create_test_index().await;

    // Create query vector - use vec_0's pattern for exact match
    let query: Vec<f32> = (0..384).map(|j| (j as f32 * 0.01)).collect();

    // Search before deletion
    let results_before = index.search(&query, 5).await.unwrap();
    assert_eq!(results_before.len(), 5);

    // Mark some vectors as deleted
    index.mark_deleted(&VectorId::from_string("vec_0")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_1")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_2")).unwrap();

    // Search after deletion - deleted vectors should not appear
    let results_after = index.search(&query, 5).await.unwrap();
    assert_eq!(results_after.len(), 5);

    // Verify none of the deleted IDs are in results
    for result in &results_after {
        assert_ne!(result.vector_id, VectorId::from_string("vec_0"));
        assert_ne!(result.vector_id, VectorId::from_string("vec_1"));
        assert_ne!(result.vector_id, VectorId::from_string("vec_2"));
    }
}

#[tokio::test]
async fn test_vacuum() {
    let mut index = create_test_index().await;

    // Mark several vectors as deleted
    index.mark_deleted(&VectorId::from_string("vec_0")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_1")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_2")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_3")).unwrap();

    // Verify they're marked deleted
    assert!(index.is_deleted(&VectorId::from_string("vec_0")));
    assert!(index.is_deleted(&VectorId::from_string("vec_1")));
    assert!(index.is_deleted(&VectorId::from_string("vec_2")));
    assert!(index.is_deleted(&VectorId::from_string("vec_3")));

    // Run vacuum
    let removed_count = index.vacuum().unwrap();
    assert_eq!(removed_count, 4);

    // After vacuum, is_deleted should still return false (they no longer exist)
    assert!(!index.is_deleted(&VectorId::from_string("vec_0")));
    assert!(!index.is_deleted(&VectorId::from_string("vec_1")));
    assert!(!index.is_deleted(&VectorId::from_string("vec_2")));
    assert!(!index.is_deleted(&VectorId::from_string("vec_3")));

    // Other vectors should still exist
    assert!(!index.is_deleted(&VectorId::from_string("vec_4")));
    assert!(!index.is_deleted(&VectorId::from_string("vec_5")));
}

#[tokio::test]
async fn test_active_count() {
    let mut index = create_test_index().await;

    // Initially should have 20 active vectors
    assert_eq!(index.active_count(), 20);

    // Mark some as deleted
    index.mark_deleted(&VectorId::from_string("vec_0")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_1")).unwrap();
    index.mark_deleted(&VectorId::from_string("vec_2")).unwrap();

    // Active count should decrease
    assert_eq!(index.active_count(), 17);

    // After vacuum, active count should still be 17
    // (vacuum physically removes them, so total count decreases)
    index.vacuum().unwrap();
    assert_eq!(index.active_count(), 17);
}

#[tokio::test]
async fn test_delete_nonexistent_vector() {
    let mut index = create_test_index().await;
    let id = VectorId::from_string("vec_nonexistent");

    // Try to delete non-existent vector
    let result = index.mark_deleted(&id);

    // Should return an error
    assert!(result.is_err());
    match result {
        Err(e) => {
            // Should be VectorNotFound error
            assert!(e.to_string().contains("not found"));
        }
        Ok(_) => panic!("Expected error for deleting non-existent vector"),
    }
}

#[tokio::test]
async fn test_delete_same_vector_twice() {
    let mut index = create_test_index().await;
    let id = VectorId::from_string("vec_5");

    // First deletion should succeed
    assert!(index.mark_deleted(&id).is_ok());
    assert!(index.is_deleted(&id));

    // Second deletion of already deleted vector
    // This should either succeed (idempotent) or fail with appropriate error
    // Based on HNSW implementation, it should fail because node doesn't exist
    let result = index.mark_deleted(&id);

    // After first deletion, the vector is marked deleted but still exists
    // So second deletion should succeed (marking an already deleted vector)
    assert!(result.is_ok());
}
