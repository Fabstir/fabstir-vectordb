// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use chrono::Utc;
use std::sync::Arc;
use vector_db::core::types::VectorId;
use vector_db::hybrid::core::{HybridConfig, HybridIndex};

/// Helper function to create a simple trained hybrid index for testing
async fn create_test_hybrid_index() -> HybridIndex {
    let config = HybridConfig {
        recent_threshold: std::time::Duration::from_secs(7 * 24 * 3600), // 7 days
        hnsw_config: vector_db::hnsw::core::HNSWConfig::default(),
        ivf_config: vector_db::ivf::core::IVFConfig {
            n_clusters: 4,
            n_probe: 4,
            train_size: 100,
            max_iterations: 10,
            seed: Some(42),
        },
        migration_batch_size: 100,
        auto_migrate: false, // Disable auto-migration for tests
    };

    let mut index = HybridIndex::new(config);

    // Generate training data (384-dim vectors) for IVF
    let training_data: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..384).map(|j| ((i + j) as f32 * 0.01)).collect())
        .collect();

    // Initialize (trains IVF index)
    index.initialize(training_data).await.unwrap();

    index
}

/// Helper to insert recent vectors (< 7 days old)
async fn insert_recent_vectors(index: &HybridIndex, count: usize) -> Vec<VectorId> {
    let mut ids = Vec::new();
    let now = Utc::now();

    for i in 0..count {
        let id = VectorId::from_string(&format!("recent_{}", i));
        let vector: Vec<f32> = (0..384).map(|j| ((i + j) as f32 * 0.01)).collect();
        let timestamp = now - chrono::Duration::hours(i as i64); // Recent (< 7 days)
        index
            .insert_with_timestamp(id.clone(), vector, timestamp)
            .await
            .unwrap();
        ids.push(id);
    }

    ids
}

/// Helper to insert historical vectors (> 7 days old)
async fn insert_historical_vectors(index: &HybridIndex, count: usize) -> Vec<VectorId> {
    let mut ids = Vec::new();
    let now = Utc::now();

    for i in 0..count {
        let id = VectorId::from_string(&format!("historical_{}", i));
        let vector: Vec<f32> = (0..384).map(|j| ((i + 100 + j) as f32 * 0.01)).collect();
        let timestamp = now - chrono::Duration::days(30 + i as i64); // Historical (> 7 days)
        index
            .insert_with_timestamp(id.clone(), vector, timestamp)
            .await
            .unwrap();
        ids.push(id);
    }

    ids
}

#[tokio::test]
async fn test_delete_from_recent_index() {
    let index = create_test_hybrid_index().await;

    // Insert recent vectors (will go to HNSW)
    let ids = insert_recent_vectors(&index, 10).await;
    let id_to_delete = ids[5].clone();

    // Verify vector exists before deletion
    let query: Vec<f32> = (0..384).map(|j| ((5 + j) as f32 * 0.01)).collect();
    let results_before = index.search(&query, 5).await.unwrap();
    assert!(
        results_before
            .iter()
            .any(|r| r.vector_id == id_to_delete),
        "Vector should exist before deletion"
    );

    // Delete from recent index
    let result = index.delete(id_to_delete.clone()).await;
    assert!(result.is_ok(), "Deletion should succeed");

    // Verify vector is marked deleted
    assert!(
        index.is_deleted(&id_to_delete).await,
        "Vector should be marked deleted"
    );

    // Verify deleted vector not in search results
    let results_after = index.search(&query, 5).await.unwrap();
    assert!(
        !results_after.iter().any(|r| r.vector_id == id_to_delete),
        "Deleted vector should not appear in search results"
    );
}

#[tokio::test]
async fn test_delete_from_historical_index() {
    let index = create_test_hybrid_index().await;

    // Insert historical vectors (will go to IVF)
    let ids = insert_historical_vectors(&index, 10).await;
    let id_to_delete = ids[5].clone();

    // Verify vector exists before deletion
    let query: Vec<f32> = (0..384).map(|j| ((105 + j) as f32 * 0.01)).collect();
    let results_before = index.search(&query, 5).await.unwrap();
    assert!(
        results_before
            .iter()
            .any(|r| r.vector_id == id_to_delete),
        "Vector should exist before deletion"
    );

    // Delete from historical index
    let result = index.delete(id_to_delete.clone()).await;
    assert!(result.is_ok(), "Deletion should succeed");

    // Verify vector is marked deleted
    assert!(
        index.is_deleted(&id_to_delete).await,
        "Vector should be marked deleted"
    );

    // Verify deleted vector not in search results
    let results_after = index.search(&query, 5).await.unwrap();
    assert!(
        !results_after.iter().any(|r| r.vector_id == id_to_delete),
        "Deleted vector should not appear in search results"
    );
}

#[tokio::test]
async fn test_delete_nonexistent_vector() {
    let index = create_test_hybrid_index().await;

    // Insert some vectors
    insert_recent_vectors(&index, 5).await;

    // Try to delete non-existent vector
    let nonexistent_id = VectorId::from_string("nonexistent");
    let result = index.delete(nonexistent_id).await;

    // Should return error
    assert!(result.is_err(), "Deleting non-existent vector should fail");
}

#[tokio::test]
async fn test_batch_delete() {
    let index = create_test_hybrid_index().await;

    // Insert mixed vectors (recent and historical)
    let recent_ids = insert_recent_vectors(&index, 5).await;
    let historical_ids = insert_historical_vectors(&index, 5).await;

    // Create batch with mixed vectors
    let ids_to_delete = vec![
        recent_ids[0].clone(),
        recent_ids[1].clone(),
        historical_ids[0].clone(),
        historical_ids[1].clone(),
        VectorId::from_string("nonexistent"), // Should fail
    ];

    let result = index.batch_delete(&ids_to_delete).await.unwrap();

    // Should successfully delete 4, fail 1
    assert_eq!(result.successful, 4, "Should delete 4 vectors");
    assert_eq!(result.failed, 1, "Should fail 1 deletion");
    assert_eq!(result.errors.len(), 1, "Should have 1 error");

    // Verify deleted vectors are marked
    assert!(index.is_deleted(&recent_ids[0]).await);
    assert!(index.is_deleted(&recent_ids[1]).await);
    assert!(index.is_deleted(&historical_ids[0]).await);
    assert!(index.is_deleted(&historical_ids[1]).await);

    // Verify non-deleted vectors still exist
    assert!(!index.is_deleted(&recent_ids[2]).await);
    assert!(!index.is_deleted(&historical_ids[2]).await);
}

#[tokio::test]
async fn test_search_excludes_deleted_vectors_both_indices() {
    let index = create_test_hybrid_index().await;

    // Insert vectors in both indices
    let recent_ids = insert_recent_vectors(&index, 10).await;
    let historical_ids = insert_historical_vectors(&index, 10).await;

    // Delete some from each index
    index.delete(recent_ids[0].clone()).await.unwrap();
    index.delete(recent_ids[1].clone()).await.unwrap();
    index.delete(historical_ids[0].clone()).await.unwrap();
    index.delete(historical_ids[1].clone()).await.unwrap();

    // Search should exclude all deleted vectors
    let query: Vec<f32> = (0..384).map(|j| (j as f32 * 0.01)).collect();
    let results = index.search(&query, 20).await.unwrap();

    // Verify no deleted vectors in results
    for result in &results {
        assert_ne!(result.vector_id, recent_ids[0]);
        assert_ne!(result.vector_id, recent_ids[1]);
        assert_ne!(result.vector_id, historical_ids[0]);
        assert_ne!(result.vector_id, historical_ids[1]);
    }
}

#[tokio::test]
async fn test_vacuum_on_hybrid_index() {
    let index = create_test_hybrid_index().await;

    // Insert vectors in both indices
    let recent_ids = insert_recent_vectors(&index, 10).await;
    let historical_ids = insert_historical_vectors(&index, 10).await;

    // Delete several vectors
    index.delete(recent_ids[0].clone()).await.unwrap();
    index.delete(recent_ids[1].clone()).await.unwrap();
    index.delete(recent_ids[2].clone()).await.unwrap();
    index.delete(historical_ids[0].clone()).await.unwrap();
    index.delete(historical_ids[1].clone()).await.unwrap();

    // Verify they're marked deleted
    assert!(index.is_deleted(&recent_ids[0]).await);
    assert!(index.is_deleted(&historical_ids[0]).await);

    // Run vacuum
    let stats = index.vacuum().await.unwrap();

    // Should have removed 5 vectors total (3 HNSW + 2 IVF)
    assert_eq!(stats.hnsw_removed, 3, "Should remove 3 from HNSW");
    assert_eq!(stats.ivf_removed, 2, "Should remove 2 from IVF");
    assert_eq!(stats.total_removed, 5, "Should remove 5 total");

    // After vacuum, is_deleted should return false (they no longer exist)
    assert!(!index.is_deleted(&recent_ids[0]).await);
    assert!(!index.is_deleted(&historical_ids[0]).await);

    // Non-deleted vectors should still exist
    assert!(!index.is_deleted(&recent_ids[3]).await);
    assert!(!index.is_deleted(&historical_ids[3]).await);
}

#[tokio::test]
async fn test_active_count() {
    let index = create_test_hybrid_index().await;

    // Initially empty
    assert_eq!(index.active_count().await, 0);

    // Insert vectors
    insert_recent_vectors(&index, 10).await;
    insert_historical_vectors(&index, 10).await;

    // Should have 20 active vectors
    assert_eq!(index.active_count().await, 20);

    // Get IDs from timestamps
    let timestamps = index.timestamps.read().await;
    let all_ids: Vec<VectorId> = timestamps.keys().cloned().collect();
    drop(timestamps);

    // Delete 5 vectors
    for id in all_ids.iter().take(5) {
        index.delete(id.clone()).await.unwrap();
    }

    // Active count should decrease to 15
    assert_eq!(index.active_count().await, 15);

    // After vacuum, active count should still be 15
    index.vacuum().await.unwrap();
    assert_eq!(index.active_count().await, 15);
}

#[tokio::test]
async fn test_delete_same_vector_twice() {
    let index = create_test_hybrid_index().await;

    // Insert a vector
    let ids = insert_recent_vectors(&index, 1).await;
    let id = ids[0].clone();

    // First deletion should succeed
    assert!(index.delete(id.clone()).await.is_ok());
    assert!(index.is_deleted(&id).await);

    // Second deletion should succeed (idempotent)
    let result = index.delete(id.clone()).await;
    assert!(result.is_ok(), "Second deletion should be idempotent");
}

#[tokio::test]
async fn test_concurrent_deletion() {
    let index = Arc::new(create_test_hybrid_index().await);

    // Insert many vectors
    let recent_ids = insert_recent_vectors(&index, 50).await;
    let historical_ids = insert_historical_vectors(&index, 50).await;

    // Spawn concurrent deletion tasks
    let mut handles = vec![];

    // Delete recent vectors concurrently
    for i in 0..10 {
        let index_clone = index.clone();
        let id = recent_ids[i].clone();
        let handle = tokio::spawn(async move {
            index_clone.delete(id).await
        });
        handles.push(handle);
    }

    // Delete historical vectors concurrently
    for i in 0..10 {
        let index_clone = index.clone();
        let id = historical_ids[i].clone();
        let handle = tokio::spawn(async move {
            index_clone.delete(id).await
        });
        handles.push(handle);
    }

    // Wait for all deletions to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent deletion should succeed");
    }

    // Verify 20 vectors deleted
    let active = index.active_count().await;
    assert_eq!(active, 80, "Should have 80 active vectors (100 - 20)");
}
