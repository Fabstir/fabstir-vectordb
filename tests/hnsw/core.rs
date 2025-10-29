// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashSet;
use vector_db::core::types::*;
use vector_db::core::vector_ops::*;
use vector_db::hnsw::core::*;

#[cfg(test)]
mod hnsw_structure_tests {
    use super::*;

    #[test]
    fn test_hnsw_node_creation() {
        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let node = HNSWNode::new(id.clone(), vector.clone());

        assert_eq!(node.id(), &id);
        assert_eq!(node.vector(), &vector);
        assert_eq!(node.level(), 0);
        assert!(node.neighbors(0).is_empty());
    }

    #[test]
    fn test_hnsw_index_initialization() {
        let config = HNSWConfig {
            max_connections: 16,
            max_connections_layer_0: 32,
            ef_construction: 200,
            seed: Some(42),
        };

        let index = HNSWIndex::new(config.clone());

        assert_eq!(index.config(), &config);
        assert_eq!(index.node_count(), 0);
        assert!(index.entry_point().is_none());
    }

    #[test]
    fn test_level_assignment() {
        let config = HNSWConfig::default();
        let index = HNSWIndex::new(config);

        // Test level distribution
        let mut level_counts = vec![0; 10];
        for _ in 0..10000 {
            let level = index.assign_level();
            if level < level_counts.len() {
                level_counts[level] += 1;
            }
        }

        // Level 0 should have most nodes (~63%)
        assert!(level_counts[0] > 6000);
        // Each higher level should have roughly half the nodes of previous
        for i in 1..5 {
            if level_counts[i] > 0 {
                let ratio = level_counts[i - 1] as f64 / level_counts[i] as f64;
                assert!(ratio > 1.5 && ratio < 2.5);
            }
        }
    }
}

#[cfg(test)]
mod hnsw_insertion_tests {
    use super::*;

    #[test]
    fn test_insert_first_node() {
        let mut index = HNSWIndex::new(HNSWConfig::default());
        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0];

        index.insert(id.clone(), vector.clone()).unwrap();

        assert_eq!(index.node_count(), 1);
        assert_eq!(index.entry_point(), Some(id.clone()));

        // First node should have level assigned
        let node = index.get_node(&id).unwrap();
        assert!(node.level() >= 0);
    }

    #[test]
    fn test_insert_multiple_nodes() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 200,
            seed: Some(42),
        });

        let vectors = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![0.5, 0.5, 0.0],
            vec![0.5, 0.0, 0.5],
        ];

        let mut ids = Vec::new();
        for vector in vectors {
            let id = VectorId::new();
            ids.push(id.clone());
            index.insert(id, vector).unwrap();
        }

        assert_eq!(index.node_count(), 5);

        // Check connectivity - each node should have neighbors
        for id in &ids {
            let node = index.get_node(id).unwrap();
            let neighbors = node.neighbors(0);
            assert!(
                !neighbors.is_empty(),
                "Node should have neighbors at layer 0"
            );
            assert!(
                neighbors.len() <= 8,
                "Should not exceed max_connections_layer_0"
            );
        }
    }

    #[test]
    fn test_neighbor_selection() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert nodes in a line: 0 -- 1 -- 2 -- 3 -- 4
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![3.0], vec![4.0]];

        let mut ids = Vec::new();
        for vector in vectors {
            let id = VectorId::new();
            ids.push(id.clone());
            index.insert(id, vector).unwrap();
        }

        // Node 2 (middle) should be connected to nearby nodes
        let node2 = index.get_node(&ids[2]).unwrap();
        let neighbors = node2.neighbors(0);

        // Should contain nodes 1 and 3 (immediate neighbors)
        let neighbor_ids: HashSet<_> = neighbors.iter().collect();
        assert!(neighbor_ids.contains(&ids[1]));
        assert!(neighbor_ids.contains(&ids[3]));
    }

    #[test]
    fn test_duplicate_insertion() {
        let mut index = HNSWIndex::new(HNSWConfig::default());
        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0];

        index.insert(id.clone(), vector.clone()).unwrap();

        // Inserting same ID should fail
        let result = index.insert(id.clone(), vector);
        assert!(result.is_err());
        match result {
            Err(HNSWError::DuplicateVector(_)) => {}
            _ => panic!("Expected DuplicateVector error"),
        }
    }
}

#[cfg(test)]
mod hnsw_search_tests {
    use super::*;

    #[test]
    fn test_search_empty_index() {
        let index = HNSWIndex::new(HNSWConfig::default());
        let query = vec![1.0, 2.0, 3.0];

        let results = index.search(&query, 5, 200).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_single_node() {
        let mut index = HNSWIndex::new(HNSWConfig::default());
        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0];

        index.insert(id.clone(), vector).unwrap();

        let query = vec![1.0, 2.0, 3.0];
        let results = index.search(&query, 1, 200).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(&results[0].vector_id, &id);
        assert!(results[0].distance < 1e-6); // Exact match
    }

    #[test]
    fn test_search_accuracy() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 16,
            max_connections_layer_0: 32,
            ef_construction: 200,
            seed: Some(42),
        });

        // Insert 100 random vectors
        let mut vectors = Vec::new();
        let mut ids = Vec::new();
        for i in 0..100 {
            let vector: Vec<f32> = (0..10).map(|j| ((i * j) as f32).sin()).collect();
            let id = VectorId::from_string(&format!("vec_{}", i));
            ids.push(id.clone());
            vectors.push(vector.clone());
            index.insert(id, vector).unwrap();
        }

        // Search for each vector - should find itself
        for (i, vector) in vectors.iter().enumerate() {
            let results = index.search(vector, 1, 200).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(&results[0].vector_id, &ids[i]);
            assert!(results[0].distance < 1e-5);
        }
    }

    #[test]
    fn test_k_nearest_neighbors() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Create vectors in 2D space for easy verification
        let vectors = vec![
            (vec![0.0, 0.0], "origin"),
            (vec![1.0, 0.0], "right"),
            (vec![0.0, 1.0], "up"),
            (vec![-1.0, 0.0], "left"),
            (vec![0.0, -1.0], "down"),
            (vec![0.5, 0.5], "up-right"),
        ];

        let mut id_map = std::collections::HashMap::new();
        for (vector, name) in vectors {
            let id = VectorId::from_string(name);
            id_map.insert(id.clone(), name);
            index.insert(id, vector).unwrap();
        }

        // Query near origin
        let query = vec![0.1, 0.1];
        let results = index.search(&query, 3, 200).unwrap();

        assert_eq!(results.len(), 3);
        // Origin should be closest
        assert_eq!(id_map[&results[0].vector_id], "origin");
        // Order of next two depends on exact distances
    }

    #[test]
    fn test_ef_parameter_impact() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 16,
            max_connections_layer_0: 32,
            ef_construction: 200,
            seed: Some(42),
        });

        // Insert many vectors
        for i in 0..500 {
            let vector: Vec<f32> = (0..20).map(|j| ((i * j) as f32).sin()).collect();
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vector).unwrap();
        }

        let query = vec![0.5; 20];

        // Search with low ef (fast but less accurate)
        let results_low_ef = index.search(&query, 10, 50).unwrap();

        // Search with high ef (slower but more accurate)
        let results_high_ef = index.search(&query, 10, 500).unwrap();

        // Both should return 10 results
        assert_eq!(results_low_ef.len(), 10);
        assert_eq!(results_high_ef.len(), 10);

        // High ef results should have better (lower) distances on average
        let avg_dist_low: f32 = results_low_ef.iter().map(|r| r.distance).sum::<f32>() / 10.0;
        let avg_dist_high: f32 = results_high_ef.iter().map(|r| r.distance).sum::<f32>() / 10.0;

        // Allow for some randomness, but high ef should generally be better
        assert!(avg_dist_high <= avg_dist_low * 1.1);
    }
}

#[cfg(test)]
mod hnsw_edge_cases {
    use super::*;

    #[test]
    fn test_search_more_k_than_nodes() {
        let mut index = HNSWIndex::new(HNSWConfig::default());

        // Insert only 3 nodes
        for i in 0..3 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32];
            index.insert(id, vector).unwrap();
        }

        // Search for 10 nearest neighbors
        let results = index.search(&vec![1.5], 10, 200).unwrap();

        // Should return only 3 results
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_multi_layer_structure() {
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 200,
            seed: Some(42), // Fixed seed for reproducibility
        });

        // Insert enough nodes to likely have multiple layers
        let mut has_multilayer = false;
        for i in 0..100 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32; 10];
            index.insert(id.clone(), vector).unwrap();

            let node = index.get_node(&id).unwrap();
            if node.level() > 0 {
                has_multilayer = true;

                // Check that higher layers have fewer connections
                let layer0_neighbors = node.neighbors(0).len();
                let layer1_neighbors = node.neighbors(1).len();

                assert!(layer1_neighbors <= layer0_neighbors);
                assert!(layer1_neighbors <= 4); // max_connections for layer > 0
            }
        }

        assert!(has_multilayer, "Should have created multi-layer structure");
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let index = Arc::new(HNSWIndex::new(HNSWConfig::default()));

        // Spawn multiple reader threads
        let mut handles = vec![];
        for i in 0..4 {
            let index_clone = Arc::clone(&index);
            let handle = thread::spawn(move || {
                let query = vec![i as f32; 10];
                // This should not crash even with empty index
                let _ = index_clone.search(&query, 5, 200);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
