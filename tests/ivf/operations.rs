// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;
use tokio;
use vector_db::core::types::*;
use vector_db::ivf::core::*;
use vector_db::ivf::operations::*;

#[cfg(test)]
mod batch_operations_tests {
    use super::*;

    #[test]
    fn test_batch_insert() {
        let mut index = create_trained_index();

        let batch = vec![
            (VectorId::from_string("a"), vec![0.1, 0.1]),
            (VectorId::from_string("b"), vec![5.0, 5.0]),
            (VectorId::from_string("c"), vec![-5.0, -5.0]),
            (VectorId::from_string("d"), vec![2.5, 2.5]),
        ];

        let result = index.batch_insert(batch).unwrap();

        assert_eq!(result.successful, 4);
        assert_eq!(result.failed, 0);
        assert!(result.errors.is_empty());
        assert_eq!(index.total_vectors(), 4);
    }

    #[test]
    fn test_batch_insert_with_failures() {
        let mut index = create_trained_index();

        // Insert one vector first
        index
            .insert(VectorId::from_string("a"), vec![0.1, 0.1])
            .unwrap();

        let batch = vec![
            (VectorId::from_string("a"), vec![0.2, 0.2]), // Duplicate
            (VectorId::from_string("b"), vec![5.0, 5.0]),
            (VectorId::from_string("c"), vec![1.0, 2.0, 3.0]), // Wrong dimension
            (VectorId::from_string("d"), vec![2.5, 2.5]),
        ];

        let result = index.batch_insert(batch).unwrap();

        assert_eq!(result.successful, 2); // b and d
        assert_eq!(result.failed, 2); // a and c
        assert_eq!(result.errors.len(), 2);
        assert_eq!(index.total_vectors(), 3); // a, b, d
    }

    #[tokio::test]
    async fn test_parallel_batch_insert() {
        let index = Arc::new(tokio::sync::RwLock::new(create_trained_index()));

        let batch1 = vec![
            (VectorId::from_string("a1"), vec![0.1, 0.0]),
            (VectorId::from_string("a2"), vec![0.2, 0.0]),
        ];

        let batch2 = vec![
            (VectorId::from_string("b1"), vec![5.0, 5.1]),
            (VectorId::from_string("b2"), vec![5.0, 5.2]),
        ];

        let index1 = Arc::clone(&index);
        let index2 = Arc::clone(&index);

        let (result1, result2) = tokio::join!(
            tokio::spawn(async move {
                let mut index = index1.write().await;
                index.batch_insert(batch1)
            }),
            tokio::spawn(async move {
                let mut index = index2.write().await;
                index.batch_insert(batch2)
            })
        );

        assert_eq!(result1.unwrap().unwrap().successful, 2);
        assert_eq!(result2.unwrap().unwrap().successful, 2);

        let index = index.read().await;
        assert_eq!(index.total_vectors(), 4);
    }

    #[test]
    fn test_batch_search() {
        let mut index = create_trained_index();

        // Insert test vectors
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let angle = i as f32 * std::f32::consts::PI / 5.0;
            index
                .insert(id, vec![angle.cos() * 3.0, angle.sin() * 3.0])
                .unwrap();
        }

        let queries = vec![vec![3.0, 0.0], vec![0.0, 3.0], vec![-3.0, 0.0]];

        let results = index.batch_search(&queries, 3).unwrap();

        assert_eq!(results.len(), 3);
        for (i, query_results) in results.iter().enumerate() {
            assert!(!query_results.is_empty());
            assert!(query_results.len() <= 3);
            // Results should be sorted by distance
            for j in 1..query_results.len() {
                assert!(query_results[j - 1].distance <= query_results[j].distance);
            }
        }
    }
}

#[cfg(test)]
mod retraining_tests {
    use super::*;

    #[test]
    fn test_retrain_with_new_config() {
        let mut index = create_trained_index();

        // Insert initial vectors
        for i in 0..50 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let angle = i as f32 * 2.0 * std::f32::consts::PI / 50.0;
            index
                .insert(id, vec![angle.cos() * 5.0, angle.sin() * 5.0])
                .unwrap();
        }

        let old_clusters = index.config().n_clusters;
        assert_eq!(old_clusters, 3);

        // Retrain with more clusters
        let new_config = IVFConfig {
            n_clusters: 10,
            n_probe: 3,
            train_size: 50,
            max_iterations: 20,
            seed: Some(42),
        };

        let result = index.retrain(new_config).unwrap();

        assert_eq!(result.old_clusters, 3);
        assert_eq!(result.new_clusters, 10);
        assert_eq!(result.vectors_reassigned, 50);
        assert!(result.converged);

        // Verify new configuration
        assert_eq!(index.config().n_clusters, 10);
        assert_eq!(index.total_vectors(), 50);

        // Search should still work
        let results = index.search(&vec![0.0, 0.0], 5).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_add_clusters() {
        let mut index = create_trained_index();

        // Insert vectors that don't fit well into existing clusters
        for i in 0..20 {
            let id = VectorId::from_string(&format!("outlier_{}", i));
            // Create outliers at (10, 10)
            index.insert(id, vec![10.0 + i as f32 * 0.1, 10.0]).unwrap();
        }

        let initial_clusters = index.config().n_clusters;

        // Add more clusters to better fit the data
        let result = index.add_clusters(2).unwrap();

        assert_eq!(result.clusters_added, 2);
        assert_eq!(result.vectors_reassigned, 20);
        assert_eq!(index.config().n_clusters, initial_clusters + 2);
    }

    #[test]
    fn test_optimize_clusters() {
        let mut index = create_trained_index();

        // Insert unevenly distributed vectors
        for i in 0..30 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            if i < 25 {
                // Most vectors near origin
                index
                    .insert(id, vec![i as f32 * 0.1, i as f32 * 0.1])
                    .unwrap();
            } else {
                // Few vectors far away
                index.insert(id, vec![20.0, 20.0]).unwrap();
            }
        }

        let stats_before = index.get_cluster_stats();

        // Optimize cluster placement
        let result = index.optimize_clusters().unwrap();

        assert!(result.iterations > 0);
        assert!(result.improvement >= 0.0);

        let stats_after = index.get_cluster_stats();

        // Variance should be improved (lower is better)
        assert!(stats_after.size_variance <= stats_before.size_variance);
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;

    #[test]
    fn test_cluster_statistics() {
        let mut index = create_trained_index();

        // Insert vectors with known distribution
        for i in 0..30 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let cluster = i % 3;
            let vector = match cluster {
                0 => vec![0.1 * i as f32, 0.1 * i as f32],
                1 => vec![5.0 + 0.1 * i as f32, 5.0],
                _ => vec![-5.0, -5.0 + 0.1 * i as f32],
            };
            index.insert(id, vector).unwrap();
        }

        let stats = index.get_cluster_stats();

        assert_eq!(stats.n_clusters, 3);
        assert_eq!(stats.total_vectors, 30);
        assert!((stats.avg_cluster_size - 10.0).abs() < 0.001);
        assert!(stats.size_variance >= 0.0);
        assert_eq!(stats.empty_clusters, 0);

        // Each cluster should have approximately 10 vectors
        let distribution = index.get_cluster_distribution();
        let mut total_in_distribution = 0;
        for (_cluster_id, size) in &distribution {
            total_in_distribution += size;
        }
        assert_eq!(total_in_distribution, 30);
    }

    #[test]
    fn test_memory_usage() {
        let mut index = create_trained_index();

        let initial_memory = index.estimate_memory_usage();
        assert!(initial_memory.total_bytes > 0);
        assert!(initial_memory.centroids_bytes > 0);
        assert_eq!(initial_memory.vectors_bytes, 0);

        // Add vectors
        for i in 0..100 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, -i as f32]).unwrap(); // 2D vectors matching training dimension
        }

        let after_memory = index.estimate_memory_usage();
        assert!(after_memory.total_bytes > initial_memory.total_bytes);
        assert!(after_memory.vectors_bytes > 0);
        assert!(after_memory.inverted_lists_bytes > 0);

        // Memory per vector
        let per_vector = (after_memory.vectors_bytes - initial_memory.vectors_bytes) / 100;
        assert!(per_vector >= 2 * 4); // At least 2 f32s per vector
    }

    #[test]
    fn test_search_quality_metrics() {
        let mut index = create_trained_index();

        // Insert test vectors
        let mut ground_truth = Vec::new();
        for i in 0..50 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32 * 0.2, (i as f32 * 0.2).sin()];
            index.insert(id.clone(), vector.clone()).unwrap();
            ground_truth.push((id, vector));
        }

        // Test search quality
        let test_queries: Vec<Vec<f32>> = (0..10).map(|i| vec![i as f32, 0.0]).collect();

        let quality = index.evaluate_search_quality(&test_queries, 5).unwrap();

        assert!(quality.avg_recall > 0.0 && quality.avg_recall <= 1.0);
        assert!(quality.avg_precision > 0.0 && quality.avg_precision <= 1.0);
        assert!(quality.avg_query_time_ms > 0.0);
        assert_eq!(quality.queries_evaluated, 10);
    }
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;

    #[test]
    fn test_compact_clusters() {
        let mut index = create_trained_index();

        // Insert vectors
        for i in 0..30 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32 * 0.1, 0.0]).unwrap();
        }

        // Delete some vectors (simulated - would need delete functionality)
        // For now, we'll test compaction on a normally populated index

        let before_memory = index.estimate_memory_usage();

        let result = index.compact_clusters().unwrap();

        assert!(result.bytes_saved >= 0);
        assert!(result.clusters_compacted <= 3);

        let after_memory = index.estimate_memory_usage();
        assert!(after_memory.total_bytes <= before_memory.total_bytes);
    }

    #[test]
    fn test_balance_clusters() {
        let mut index = create_trained_index();

        // Create imbalanced distribution
        // Most vectors go to cluster around (0,0)
        for i in 0..35 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            // All vectors very close to (0,0)
            let vector = vec![i as f32 * 0.001, i as f32 * 0.001];
            index.insert(id, vector).unwrap();
        }

        // Just a few vectors for other clusters
        for i in 35..38 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![5.0 + (i - 35) as f32 * 0.01, 5.0];
            index.insert(id, vector).unwrap();
        }

        for i in 38..40 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![-5.0, -5.0 + (i - 38) as f32 * 0.01];
            index.insert(id, vector).unwrap();
        }

        let stats_before = index.get_cluster_stats();
        assert!(stats_before.size_variance > 0.0);

        // Balance the clusters
        let result = index.balance_clusters(0.2).unwrap(); // 20% threshold

        // Either vectors were moved or balance was already good
        assert!(result.vectors_moved >= 0);
        // If vectors were moved, balance should have improved
        if result.vectors_moved > 0 {
            assert!(result.balance_improved);
        }

        let stats_after = index.get_cluster_stats();
        // Variance should be improved or at least not worse
        assert!(stats_after.size_variance <= stats_before.size_variance);
    }

    #[test]
    fn test_export_import_centroids() {
        let index = create_trained_index();

        // Export centroids
        let centroids_data = index.export_centroids().unwrap();

        assert_eq!(centroids_data.len(), 3); // 3 clusters
        assert_eq!(centroids_data[0].dimension, 2);

        // Create new index and import
        let mut new_index = IVFIndex::new(index.config().clone());
        new_index.import_centroids(centroids_data).unwrap();

        assert!(new_index.is_trained());
        assert_eq!(new_index.dimension(), Some(2));

        // Should be able to insert vectors
        new_index
            .insert(VectorId::from_string("test"), vec![1.0, 1.0])
            .unwrap();
    }
}

// Helper function
fn create_trained_index() -> IVFIndex {
    let config = IVFConfig {
        n_clusters: 3,
        n_probe: 2,
        train_size: 9,
        max_iterations: 10,
        seed: Some(42),
    };

    let mut index = IVFIndex::new(config);

    let training_data = vec![
        vec![0.0, 0.0],
        vec![0.1, 0.1],
        vec![0.2, -0.1],
        vec![5.0, 5.0],
        vec![5.1, 4.9],
        vec![4.9, 5.1],
        vec![-5.0, -5.0],
        vec![-4.9, -5.1],
        vec![-5.1, -4.9],
    ];

    index.train(&training_data).unwrap();
    index
}
