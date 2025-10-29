// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashSet;
use vector_db::core::types::*;
use vector_db::core::vector_ops::*;
use vector_db::ivf::core::*;

#[cfg(test)]
mod ivf_structure_tests {
    use super::*;

    #[test]
    fn test_ivf_config_creation() {
        let config = IVFConfig {
            n_clusters: 100,
            n_probe: 10,
            train_size: 10000,
            max_iterations: 25,
            seed: Some(42),
        };

        assert_eq!(config.n_clusters, 100);
        assert_eq!(config.n_probe, 10);
        assert!(config.is_valid());
    }

    #[test]
    fn test_invalid_config() {
        let config = IVFConfig {
            n_clusters: 0, // Invalid
            n_probe: 10,
            train_size: 100,
            max_iterations: 25,
            seed: None,
        };

        assert!(!config.is_valid());
    }

    #[test]
    fn test_centroid_creation() {
        let id = ClusterId(5);
        let vector = vec![1.0, 2.0, 3.0];
        let centroid = Centroid::new(id, vector.clone());

        assert_eq!(centroid.id(), id);
        assert_eq!(centroid.vector(), &vector);
        assert_eq!(centroid.dimension(), 3);
    }

    #[test]
    fn test_ivf_index_initialization() {
        let config = IVFConfig::default();
        let index = IVFIndex::new(config.clone());

        assert_eq!(index.config(), &config);
        assert!(!index.is_trained());
        assert_eq!(index.dimension(), None);
        assert_eq!(index.total_vectors(), 0);
    }
}

#[cfg(test)]
mod ivf_training_tests {
    use super::*;

    #[test]
    fn test_train_simple_2d() {
        let config = IVFConfig {
            n_clusters: 3,
            n_probe: 2,
            train_size: 9,
            max_iterations: 10,
            seed: Some(42),
        };

        let mut index = IVFIndex::new(config);

        // Create simple 2D points in 3 clear clusters
        let training_data = vec![
            // Cluster 1 (around 0,0)
            vec![0.0, 0.0],
            vec![0.1, 0.1],
            vec![0.2, -0.1],
            // Cluster 2 (around 5,5)
            vec![5.0, 5.0],
            vec![5.1, 4.9],
            vec![4.9, 5.1],
            // Cluster 3 (around -5,-5)
            vec![-5.0, -5.0],
            vec![-4.9, -5.1],
            vec![-5.1, -4.9],
        ];

        let result = index.train(&training_data).unwrap();

        assert!(index.is_trained());
        assert_eq!(index.dimension(), Some(2));
        assert_eq!(result.iterations, 10);
        assert!(result.converged);
        assert!(result.final_error < 1.0); // Should converge well

        // Check centroids are roughly where we expect
        let centroids = index.get_centroids();
        assert_eq!(centroids.len(), 3);

        // Each centroid should be near one of the cluster centers
        let expected_centers = vec![vec![0.0, 0.0], vec![5.0, 5.0], vec![-5.0, -5.0]];

        for centroid in centroids {
            let mut found_close = false;
            for expected in &expected_centers {
                let dist = euclidean_distance_scalar(centroid.vector(), expected);
                if dist < 1.0 {
                    found_close = true;
                    break;
                }
            }
            assert!(found_close, "Centroid not near any expected center");
        }
    }

    #[test]
    fn test_train_convergence() {
        let config = IVFConfig {
            n_clusters: 4,
            n_probe: 2,
            train_size: 100,
            max_iterations: 50,
            seed: Some(42),
        };

        let mut index = IVFIndex::new(config);

        // Generate random training data
        let training_data: Vec<Vec<f32>> = (0..100)
            .map(|i| {
                vec![
                    (i as f32 * 0.1).sin() * 10.0,
                    (i as f32 * 0.1).cos() * 10.0,
                    (i as f32 * 0.1) % 10.0,
                ]
            })
            .collect();

        let result = index.train(&training_data).unwrap();

        assert!(index.is_trained());
        assert_eq!(index.dimension(), Some(3));
        assert!(result.iterations <= 50);

        // Error should decrease over iterations
        assert!(result.initial_error > result.final_error);
    }

    #[test]
    fn test_train_empty_data() {
        let mut index = IVFIndex::new(IVFConfig::default());
        let result = index.train(&Vec::<Vec<f32>>::new());

        assert!(result.is_err());
        match result {
            Err(IVFError::InsufficientTrainingData { .. }) => {}
            _ => panic!("Expected InsufficientTrainingData error"),
        }
    }

    #[test]
    fn test_train_insufficient_data() {
        let config = IVFConfig {
            n_clusters: 10,
            n_probe: 3,
            train_size: 100,
            max_iterations: 25,
            seed: None,
        };

        let mut index = IVFIndex::new(config);

        // Only 5 vectors for 10 clusters
        let training_data = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![-1.0, 0.0],
            vec![0.0, -1.0],
            vec![0.0, 0.0],
        ];

        let result = index.train(&training_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_train_mismatched_dimensions() {
        let config = IVFConfig {
            n_clusters: 2, // Only 2 clusters so 3 vectors is enough
            n_probe: 1,
            train_size: 10,
            max_iterations: 10,
            seed: None,
        };
        let mut index = IVFIndex::new(config);

        let training_data = vec![
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0], // Wrong dimension
            vec![6.0, 7.0, 8.0],
        ];

        let result = index.train(&training_data);
        assert!(result.is_err());
        match result {
            Err(IVFError::InconsistentDimensions { .. }) => {}
            _ => panic!("Expected InconsistentDimensions error"),
        }
    }
}

#[cfg(test)]
mod ivf_insertion_tests {
    use super::*;

    #[test]
    fn test_insert_untrained() {
        let mut index = IVFIndex::new(IVFConfig::default());

        let result = index.insert(VectorId::new(), vec![1.0, 2.0]);
        assert!(result.is_err());
        match result {
            Err(IVFError::NotTrained) => {}
            _ => panic!("Expected NotTrained error"),
        }
    }

    #[test]
    fn test_insert_single_vector() {
        let mut index = create_trained_index();

        let id = VectorId::from_string("test");
        let vector = vec![1.0, 1.0];

        index.insert(id.clone(), vector.clone()).unwrap();

        assert_eq!(index.total_vectors(), 1);

        // Should be assigned to nearest cluster
        let cluster_id = index.find_cluster(&vector).unwrap();
        assert!(index.get_cluster_size(cluster_id) > 0);
    }

    #[test]
    fn test_insert_multiple_vectors() {
        let mut index = create_trained_index();

        let vectors = vec![
            ("a", vec![0.0, 0.0]),
            ("b", vec![5.0, 5.0]),
            ("c", vec![-5.0, -5.0]),
            ("d", vec![2.5, 2.5]),
            ("e", vec![-2.5, -2.5]),
        ];

        for (name, vector) in vectors {
            let id = VectorId::from_string(name);
            index.insert(id, vector).unwrap();
        }

        assert_eq!(index.total_vectors(), 5);

        // Check cluster distribution
        let distribution = index.get_cluster_distribution();
        assert_eq!(distribution.len(), 3); // 3 clusters
        assert_eq!(distribution.values().sum::<usize>(), 5); // 5 total vectors
    }

    #[test]
    fn test_insert_duplicate() {
        let mut index = create_trained_index();

        let id = VectorId::from_string("dup");
        let vector = vec![1.0, 1.0];

        index.insert(id.clone(), vector.clone()).unwrap();

        // Inserting same ID should fail
        let result = index.insert(id, vector);
        assert!(result.is_err());
        match result {
            Err(IVFError::DuplicateVector(_)) => {}
            _ => panic!("Expected DuplicateVector error"),
        }
    }

    #[test]
    fn test_insert_wrong_dimension() {
        let mut index = create_trained_index();

        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0]; // 3D instead of 2D

        let result = index.insert(id, vector);
        assert!(result.is_err());
        match result {
            Err(IVFError::DimensionMismatch { .. }) => {}
            _ => panic!("Expected DimensionMismatch error"),
        }
    }
}

#[cfg(test)]
mod ivf_search_tests {
    use super::*;

    #[test]
    fn test_search_empty_index() {
        let index = create_trained_index();
        let query = vec![1.0, 1.0];

        let results = index.search(&query, 5).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_single_cluster() {
        let mut index = create_trained_index();

        // Insert vectors all in same cluster
        for i in 0..5 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![0.1 * i as f32, 0.1 * i as f32];
            index.insert(id, vector).unwrap();
        }

        let query = vec![0.25, 0.25];
        let results = index.search(&query, 3).unwrap();

        assert_eq!(results.len(), 3);
        // Results should be ordered by distance
        for i in 1..results.len() {
            assert!(results[i - 1].distance <= results[i].distance);
        }
    }

    #[test]
    fn test_search_multi_probe() {
        let config = IVFConfig {
            n_clusters: 3,
            n_probe: 2, // Search 2 clusters
            train_size: 9,
            max_iterations: 10,
            seed: Some(42),
        };

        let mut index = IVFIndex::new(config);
        train_simple_index(&mut index);

        // Insert vectors across clusters
        let vectors = vec![
            ("a", vec![0.0, 0.0]),   // Cluster 1
            ("b", vec![5.0, 5.0]),   // Cluster 2
            ("c", vec![-5.0, -5.0]), // Cluster 3
            ("d", vec![2.5, 2.5]),   // Between 1 and 2
        ];

        for (name, vector) in vectors {
            index.insert(VectorId::from_string(name), vector).unwrap();
        }

        // Query between clusters 1 and 2
        let query = vec![2.5, 2.5];
        let results = index.search(&query, 4).unwrap();

        // With n_probe=2, we search 2 clusters, so might not find all 4
        assert!(results.len() >= 3);
        assert!(results.len() <= 4);

        // With n_probe=2, should find vectors from 2 nearest clusters
        let found_ids: HashSet<String> = results.iter().map(|r| r.vector_id.to_string()).collect();

        // Should definitely find "d" (exact match)
        assert!(found_ids.contains(&VectorId::from_string("d").to_string()));
    }

    #[test]
    fn test_search_more_k_than_vectors() {
        let mut index = create_trained_index();

        // Insert only 3 vectors
        for i in 0..3 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, i as f32]).unwrap();
        }

        // Search for 10
        let results = index.search(&vec![1.5, 1.5], 10).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_exact_match() {
        let mut index = create_trained_index();

        let id = VectorId::from_string("exact");
        let vector = vec![3.14159, 2.71828];
        index.insert(id.clone(), vector.clone()).unwrap();

        let results = index.search(&vector, 1).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].vector_id, id);
        assert!(results[0].distance < 1e-6);
    }

    #[test]
    fn test_custom_n_probe() {
        let mut index = create_trained_index();

        // Insert vectors
        for i in 0..20 {
            let angle = i as f32 * std::f32::consts::PI / 10.0;
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![angle.cos() * 5.0, angle.sin() * 5.0];
            index.insert(id, vector).unwrap();
        }

        let query = vec![0.0, 0.0];

        // Search with n_probe=1
        let results_1 = index.search_with_config(&query, 10, 1).unwrap();

        // Search with n_probe=3
        let results_3 = index.search_with_config(&query, 10, 3).unwrap();

        // More probes should generally find better results
        assert!(results_1.len() <= results_3.len());
    }
}

// Helper functions
fn create_trained_index() -> IVFIndex {
    let config = IVFConfig {
        n_clusters: 3,
        n_probe: 2,
        train_size: 9,
        max_iterations: 10,
        seed: Some(42),
    };

    let mut index = IVFIndex::new(config);
    train_simple_index(&mut index);
    index
}

fn train_simple_index(index: &mut IVFIndex) {
    let training_data = vec![
        // Cluster 1
        vec![0.0, 0.0],
        vec![0.1, 0.1],
        vec![0.2, -0.1],
        // Cluster 2
        vec![5.0, 5.0],
        vec![5.1, 4.9],
        vec![4.9, 5.1],
        // Cluster 3
        vec![-5.0, -5.0],
        vec![-4.9, -5.1],
        vec![-5.1, -4.9],
    ];

    index.train(&training_data).unwrap();
}
