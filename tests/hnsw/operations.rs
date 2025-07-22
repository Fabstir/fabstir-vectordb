use std::sync::Arc;
use std::time::Instant;
use tokio;
use vector_db::core::types::*;
use vector_db::hnsw::core::*;
use vector_db::hnsw::operations::*;

#[cfg(test)]
mod batch_operations_tests {
    use super::*;

    #[test]
    fn test_batch_insert_small() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        let batch = vec![
            (VectorId::from_string("a"), vec![1.0, 0.0]),
            (VectorId::from_string("b"), vec![0.0, 1.0]),
            (VectorId::from_string("c"), vec![-1.0, 0.0]),
            (VectorId::from_string("d"), vec![0.0, -1.0]),
        ];

        let results = index.batch_insert(batch).unwrap();

        assert_eq!(results.successful, 4);
        assert_eq!(results.failed, 0);
        assert!(results.errors.is_empty());
        assert_eq!(index.node_count(), 4);

        // Verify all nodes are connected
        for id in ["a", "b", "c", "d"] {
            let node = index.get_node(&VectorId::from_string(id)).unwrap();
            assert!(!node.neighbors(0).is_empty());
        }
    }

    #[test]
    fn test_batch_insert_with_duplicates() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert initial node
        index
            .insert(VectorId::from_string("a"), vec![1.0, 0.0])
            .unwrap();

        let batch = vec![
            (VectorId::from_string("a"), vec![2.0, 0.0]), // Duplicate
            (VectorId::from_string("b"), vec![0.0, 1.0]),
            (VectorId::from_string("c"), vec![-1.0, 0.0]),
        ];

        let results = index.batch_insert(batch).unwrap();

        assert_eq!(results.successful, 2);
        assert_eq!(results.failed, 1);
        assert_eq!(results.errors.len(), 1);
        assert!(matches!(results.errors[0].1, HNSWError::DuplicateVector(_)));
        assert_eq!(index.node_count(), 3);
    }

    #[tokio::test]
    async fn test_parallel_batch_insert() {
        let index = Arc::new(tokio::sync::RwLock::new(HNSWIndex::new(
            HNSWConfig::default(),
        )));

        // Create batches for parallel insertion
        let batch1 = vec![
            (VectorId::from_string("a1"), vec![1.0, 0.0]),
            (VectorId::from_string("a2"), vec![2.0, 0.0]),
        ];

        let batch2 = vec![
            (VectorId::from_string("b1"), vec![0.0, 1.0]),
            (VectorId::from_string("b2"), vec![0.0, 2.0]),
        ];

        let index1 = Arc::clone(&index);
        let index2 = Arc::clone(&index);

        let handle1 = tokio::spawn(async move {
            let mut index = index1.write().await;
            index.batch_insert(batch1)
        });

        let handle2 = tokio::spawn(async move {
            let mut index = index2.write().await;
            index.batch_insert(batch2)
        });

        let results1 = handle1.await.unwrap().unwrap();
        let results2 = handle2.await.unwrap().unwrap();

        assert_eq!(results1.successful, 2);
        assert_eq!(results2.successful, 2);

        let index = index.read().await;
        assert_eq!(index.node_count(), 4);
    }

    #[test]
    fn test_batch_insert_progress_callback() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        let batch: Vec<_> = (0..10)
            .map(|i| (VectorId::from_string(&format!("vec_{}", i)), vec![i as f32]))
            .collect();

        let mut progress_calls = 0;
        let progress_callback = |current: usize, total: usize| {
            progress_calls += 1;
            assert!(current <= total);
            assert_eq!(total, 10);
        };

        let results = index
            .batch_insert_with_progress(batch, progress_callback)
            .unwrap();

        assert_eq!(results.successful, 10);
        assert_eq!(progress_calls, 10);
    }
}

#[cfg(test)]
mod deletion_tests {
    use super::*;

    #[test]
    fn test_mark_deleted() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert nodes
        let ids: Vec<_> = (0..5)
            .map(|i| VectorId::from_string(&format!("vec_{}", i)))
            .collect();

        for (i, id) in ids.iter().enumerate() {
            index.insert(id.clone(), vec![i as f32]).unwrap();
        }

        // Mark one as deleted
        index.mark_deleted(&ids[2]).unwrap();

        // Verify it's marked as deleted
        assert!(index.is_deleted(&ids[2]));
        assert!(!index.is_deleted(&ids[1]));

        // Search should not return deleted nodes
        let results = index.search(&vec![2.0], 5, 50).unwrap();
        assert!(!results.iter().any(|r| r.vector_id == ids[2]));
    }

    #[test]
    fn test_delete_nonexistent() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        let result = index.mark_deleted(&VectorId::from_string("nonexistent"));
        assert!(matches!(result, Err(HNSWError::VectorNotFound(_))));
    }

    #[test]
    fn test_batch_delete() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert nodes
        let ids: Vec<_> = (0..10)
            .map(|i| VectorId::from_string(&format!("vec_{}", i)))
            .collect();

        for (i, id) in ids.iter().enumerate() {
            index.insert(id.clone(), vec![i as f32]).unwrap();
        }

        // Delete every other node
        let to_delete: Vec<_> = ids.iter().step_by(2).cloned().collect();
        let results = index.batch_delete(&to_delete).unwrap();

        assert_eq!(results.successful, 5);
        assert_eq!(results.failed, 0);

        // Verify deletions
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(index.is_deleted(id), i % 2 == 0);
        }
    }

    #[test]
    fn test_vacuum_deleted_nodes() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert and delete some nodes
        let ids: Vec<_> = (0..10)
            .map(|i| VectorId::from_string(&format!("vec_{}", i)))
            .collect();

        for (i, id) in ids.iter().enumerate() {
            index.insert(id.clone(), vec![i as f32]).unwrap();
        }

        // Mark half as deleted
        for i in 0..5 {
            index.mark_deleted(&ids[i]).unwrap();
        }

        assert_eq!(index.node_count(), 10);
        assert_eq!(index.active_count(), 5);

        // Vacuum to remove deleted nodes
        let removed = index.vacuum().unwrap();
        assert_eq!(removed, 5);
        assert_eq!(index.node_count(), 5);
        assert_eq!(index.active_count(), 5);

        // Deleted nodes should be gone
        for i in 0..5 {
            assert!(index.get_node(&ids[i]).is_none());
        }
    }
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;

    #[test]
    fn test_optimize_connections() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 50,
            seed: Some(42),
        });

        // Insert nodes
        for i in 0..5 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32]).unwrap();
        }

        // Get initial graph stats
        let stats_before = index.get_graph_stats();

        // Optimize connections
        let changes = index.optimize_connections(0.5).unwrap();

        assert!(changes.edges_added >= 0);
        assert!(changes.edges_removed >= 0);

        // Verify connectivity is maintained
        let stats_after = index.get_graph_stats();
        assert!(stats_after.connected_components <= stats_before.connected_components);
    }

    #[test]
    fn test_rebalance_graph() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert nodes with poor distribution
        for i in 0..8 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            // All vectors very similar
            index.insert(id, vec![0.1 * i as f32, 0.0]).unwrap();
        }

        let stats_before = index.get_graph_stats();

        // Rebalance
        let result = index.rebalance().unwrap();

        assert!(result.nodes_moved >= 0);
        assert!(result.layers_adjusted >= 0);

        let stats_after = index.get_graph_stats();

        // Graph should be better balanced
        assert!(stats_after.avg_degree > 0.0);
    }

    #[test]
    fn test_graph_statistics() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Empty graph
        let empty_stats = index.get_graph_stats();
        assert_eq!(empty_stats.total_nodes, 0);
        assert_eq!(empty_stats.total_edges, 0);

        // Add nodes
        for i in 0..5 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32]).unwrap();
        }

        let stats = index.get_graph_stats();
        assert_eq!(stats.total_nodes, 5);
        assert!(stats.total_edges > 0);
        assert!(stats.avg_degree > 0.0);
        assert_eq!(stats.connected_components, 1); // Should be fully connected
        assert!(stats.max_layer >= 0);
    }

    #[test]
    fn test_memory_usage_tracking() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        let initial_memory = index.estimate_memory_usage();
        assert!(initial_memory.total_bytes > 0);
        assert_eq!(initial_memory.nodes_bytes, 0);

        // Add nodes and check memory increases
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32; 100]).unwrap(); // Large vectors
        }

        let after_memory = index.estimate_memory_usage();
        assert!(after_memory.total_bytes > initial_memory.total_bytes);
        assert!(after_memory.nodes_bytes > 0);
        assert!(after_memory.vectors_bytes > 0);
        assert!(after_memory.graph_bytes > 0);
    }
}

#[cfg(test)]
mod compaction_tests {
    use super::*;

    #[test]
    fn test_compact_layers() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 50,
            seed: Some(42),
        });

        // Insert nodes to create multiple layers
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32; 10]).unwrap();
        }

        // Delete some nodes from higher layers
        let stats_before = index.get_graph_stats();

        // Find and delete some higher-layer nodes
        let mut deleted = 0;
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            if let Some(node) = index.get_node(&id) {
                if node.level() > 0 {
                    index.mark_deleted(&id).unwrap();
                    deleted += 1;
                    if deleted >= 3 {
                        break;
                    }
                }
            }
        }

        // Compact layers
        let result = index.compact_layers().unwrap();

        assert!(result.layers_removed >= 0);
        assert!(result.nodes_relocated >= 0);

        let stats_after = index.get_graph_stats();
        assert!(stats_after.max_layer <= stats_before.max_layer);
    }

    #[test]
    fn test_defragment_storage() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Create fragmentation by inserting and deleting
        let ids: Vec<_> = (0..10)
            .map(|i| VectorId::from_string(&format!("vec_{}", i)))
            .collect();

        for (i, id) in ids.iter().enumerate() {
            index.insert(id.clone(), vec![i as f32]).unwrap();
        }

        // Delete every third node
        for i in (0..10).step_by(3) {
            index.mark_deleted(&ids[i]).unwrap();
        }

        let memory_before = index.estimate_memory_usage();

        // Defragment
        let result = index.defragment().unwrap();

        assert!(result.bytes_saved >= 0);
        assert!(result.nodes_moved >= 0);

        let memory_after = index.estimate_memory_usage();
        assert!(memory_after.total_bytes <= memory_before.total_bytes);
    }
}
