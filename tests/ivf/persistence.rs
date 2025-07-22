use vector_db::ivf::core::*;
use vector_db::ivf::persistence::*;
use vector_db::core::types::*;
use vector_db::core::storage::*;
use std::collections::HashMap;
use tokio;

#[cfg(test)]
mod ivf_serialization_tests {
    use super::*;

    #[test]
    fn test_centroid_serialization() {
        let id = ClusterId(42);
        let vector = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let centroid = Centroid::new(id, vector);
        
        let serialized = centroid.to_cbor().unwrap();
        let deserialized = Centroid::from_cbor(&serialized).unwrap();
        
        assert_eq!(deserialized.id(), id);
        assert_eq!(deserialized.vector(), centroid.vector());
        assert_eq!(deserialized.dimension(), 5);
    }

    #[test]
    fn test_ivf_metadata_serialization() {
        let config = IVFConfig {
            n_clusters: 100,
            n_probe: 10,
            train_size: 10000,
            max_iterations: 25,
            seed: Some(42),
        };
        
        let metadata = IVFMetadata {
            version: 1,
            config: config.clone(),
            dimension: 128,
            n_vectors: 50000,
            centroids_count: 100,
            timestamp: chrono::Utc::now(),
        };
        
        let serialized = metadata.to_cbor().unwrap();
        let deserialized = IVFMetadata::from_cbor(&serialized).unwrap();
        
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.config.n_clusters, 100);
        assert_eq!(deserialized.dimension, 128);
        assert_eq!(deserialized.n_vectors, 50000);
    }

    #[test]
    fn test_inverted_list_serialization() {
        let mut list = SerializableInvertedList {
            cluster_id: ClusterId(5),
            vectors: HashMap::new(),
        };
        
        // Add some vectors
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32; 20];
            list.vectors.insert(id, vector);
        }
        
        let serialized = list.to_cbor().unwrap();
        let deserialized = SerializableInvertedList::from_cbor(&serialized).unwrap();
        
        assert_eq!(deserialized.cluster_id(), ClusterId(5));
        assert_eq!(deserialized.size(), 10);
        
        // Verify all vectors are preserved
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = deserialized.vectors.get(&id);
            assert!(vector.is_some());
            assert_eq!(vector.unwrap().len(), 20);
        }
    }

    #[test]
    fn test_compressed_inverted_list() {
        let mut list = SerializableInvertedList {
            cluster_id: ClusterId(1),
            vectors: HashMap::new(),
        };
        
        // Add vectors with repetitive patterns (good for compression)
        for i in 0..100 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![0.1 * (i % 10) as f32; 50];
            list.vectors.insert(id, vector);
        }
        
        let uncompressed = list.to_cbor().unwrap();
        let compressed = list.to_cbor_compressed().unwrap();
        
        // Compressed should be smaller
        assert!(compressed.len() < uncompressed.len());
        
        // Should decompress correctly
        let decompressed = SerializableInvertedList::from_cbor_compressed(&compressed).unwrap();
        assert_eq!(decompressed.size(), 100);
    }
}

#[cfg(test)]
mod ivf_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_save_empty_index() {
        let storage = MockS5Storage::new();
        let index = create_trained_index();
        let persister = IVFPersister::new(storage);
        
        persister.save_index(&index, "/test/ivf").await.unwrap();
        
        // Check files were created
        let files = persister.storage().list("/test/ivf/").await.unwrap();
        assert!(files.iter().any(|f| f.contains("metadata.cbor")));
        assert!(files.iter().any(|f| f.contains("centroids.cbor")));
    }

    #[tokio::test]
    async fn test_save_and_load_small_index() {
        let storage = MockS5Storage::new();
        let mut index = create_trained_index();
        
        // Insert some vectors
        let vectors = vec![
            ("a", vec![0.1, 0.1]),
            ("b", vec![5.0, 5.0]),
            ("c", vec![-5.0, -5.0]),
            ("d", vec![2.5, 2.5]),
            ("e", vec![-2.5, -2.5]),
        ];
        
        for (name, vector) in &vectors {
            index.insert(VectorId::from_string(name), vector.clone()).unwrap();
        }
        
        let persister = IVFPersister::new(storage);
        
        // Save
        persister.save_index(&index, "/test/ivf").await.unwrap();
        
        // Load
        let loaded = persister.load_index("/test/ivf").await.unwrap();
        
        assert_eq!(loaded.total_vectors(), 5);
        assert_eq!(loaded.dimension(), Some(2));
        assert!(loaded.is_trained());
        
        // Test search works
        let results = loaded.search(&vec![0.0, 0.0], 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_save_large_index_with_chunks() {
        let storage = MockS5Storage::new();
        let mut index = create_large_trained_index(30);
        
        // Insert many vectors
        for i in 0..300 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let angle = i as f32 * 0.1;
            let vector = vec![angle.sin() * 10.0, angle.cos() * 10.0];
            index.insert(id, vector).unwrap();
        }
        
        let persister = IVFPersister::with_chunk_size(storage, 10);
        
        // Save
        persister.save_index(&index, "/test/large_ivf").await.unwrap();
        
        // Check chunks were created
        let files = persister.storage().list("/test/large_ivf/inverted_lists/").await.unwrap();
        let chunk_files: Vec<_> = files.iter()
            .filter(|f| f.contains("chunk_"))
            .collect();
        
        // Should have multiple chunks (30 clusters / 10 per chunk = 3)
        assert!(chunk_files.len() >= 3);
        
        // Load and verify
        let loaded = persister.load_index("/test/large_ivf").await.unwrap();
        assert_eq!(loaded.total_vectors(), 300);
        assert_eq!(loaded.config().n_clusters, 30);
    }

    #[tokio::test]
    async fn test_incremental_save() {
        let storage = MockS5Storage::new();
        let mut index = create_trained_index();
        let persister = IVFPersister::new(storage);
        
        // Initial save
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, i as f32]).unwrap();
        }
        persister.save_index(&index, "/test/incremental").await.unwrap();
        
        // Track modified clusters
        let mut modified_clusters = HashMap::new();
        
        // Add more vectors
        for i in 10..15 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32, i as f32];
            let cluster_id = index.find_cluster(&vector).unwrap();
            index.insert(id, vector).unwrap();
            if let Some(list) = index.get_inverted_list(cluster_id) {
                modified_clusters.insert(cluster_id, SerializableInvertedList::from_inverted_list(cluster_id, list));
            }
        }
        
        // Incremental save
        persister.save_incremental(&index, "/test/incremental", &modified_clusters).await.unwrap();
        
        // Load and verify
        let loaded = persister.load_index("/test/incremental").await.unwrap();
        assert_eq!(loaded.total_vectors(), 15);
    }

    #[tokio::test]
    async fn test_versioning() {
        let storage = MockS5Storage::new();
        
        // Create metadata with future version
        let future_metadata = IVFMetadata {
            version: 999,
            config: IVFConfig::default(),
            dimension: 128,
            n_vectors: 0,
            centroids_count: 100,
            timestamp: chrono::Utc::now(),
        };
        
        storage.put("/future/ivf/metadata.cbor", future_metadata.to_cbor().unwrap())
            .await.unwrap();
        
        let persister = IVFPersister::new(storage);
        let result = persister.load_index("/future/ivf").await;
        
        assert!(result.is_err());
        match result {
            Err(PersistenceError::IncompatibleVersion { .. }) => {},
            _ => panic!("Expected IncompatibleVersion error"),
        }
    }

    #[tokio::test]
    async fn test_recovery_from_partial_save() {
        let storage = MockS5Storage::new();
        let mut index = create_trained_index();
        
        // Insert vectors
        for i in 0..20 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, -i as f32]).unwrap();
        }
        
        // Simulate partial save - only save metadata and centroids
        let metadata = IVFMetadata::from_index(&index);
        storage.put("/partial/ivf/metadata.cbor", metadata.to_cbor().unwrap())
            .await.unwrap();
        
        let centroids = index.get_centroids();
        storage.put("/partial/ivf/centroids.cbor", serialize_centroids(&centroids).unwrap())
            .await.unwrap();
        
        // Don't save inverted lists
        
        let persister = IVFPersister::new(storage);
        let result = persister.load_index("/partial/ivf").await;
        
        // Should detect incomplete save
        assert!(result.is_err());
        
        // Check integrity
        let integrity = persister.check_integrity("/partial/ivf").await.unwrap();
        assert_eq!(integrity.expected_vectors, 20);
        assert_eq!(integrity.found_vectors, 0); // No inverted lists saved
        assert!(!integrity.is_complete);
    }

    #[tokio::test]
    async fn test_compression_options() {
        let storage = MockS5Storage::new();
        let mut index = create_trained_index();
        
        // Insert repetitive data (good for compression)
        for i in 0..100 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let pattern = (i % 10) as f32;
            index.insert(id, vec![pattern, pattern]).unwrap();
        }
        
        // Save without compression
        let persister_no_compress = IVFPersister::new(storage.clone());
        persister_no_compress.save_index(&index, "/test/no_compress").await.unwrap();
        
        // Save with compression
        let persister_compress = IVFPersister::with_compression(storage.clone(), true);
        persister_compress.save_index(&index, "/test/compress").await.unwrap();
        
        // Compare sizes
        let files_no_compress = persister_no_compress.storage()
            .list("/test/no_compress/").await.unwrap();
        let files_compress = persister_compress.storage()
            .list("/test/compress/").await.unwrap();
        
        // Compressed version should have smaller total size
        let size_no_compress = calculate_total_size(persister_no_compress.storage(), &files_no_compress).await;
        let size_compress = calculate_total_size(persister_compress.storage(), &files_compress).await;
        
        assert!(size_compress < size_no_compress);
        
        // Both should load correctly
        let loaded_no_compress = persister_no_compress.load_index("/test/no_compress").await.unwrap();
        let loaded_compress = persister_compress.load_index("/test/compress").await.unwrap();
        
        assert_eq!(loaded_no_compress.total_vectors(), 100);
        assert_eq!(loaded_compress.total_vectors(), 100);
    }
}

#[cfg(test)]
mod ivf_migration_tests {
    use super::*;

    #[tokio::test]
    async fn test_retrain_and_migrate() {
        let storage = MockS5Storage::new();
        let persister = IVFPersister::new(storage);
        
        // Create initial index with 3 clusters
        let mut index = IVFIndex::new(IVFConfig {
            n_clusters: 3,
            n_probe: 2,
            train_size: 9,
            max_iterations: 10,
            seed: Some(42),
        });
        
        train_simple_index(&mut index);
        
        // Insert initial vectors
        for i in 0..30 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let angle = i as f32 * 2.0 * std::f32::consts::PI / 30.0;
            index.insert(id, vec![angle.cos() * 5.0, angle.sin() * 5.0]).unwrap();
        }
        
        persister.save_index(&index, "/test/v1").await.unwrap();
        
        // Create new index with more clusters
        let new_config = IVFConfig {
            n_clusters: 10,
            n_probe: 3,
            train_size: 30,
            max_iterations: 20,
            seed: Some(42),
        };
        
        // Migrate data
        let migration_result = persister.migrate_index(
            "/test/v1",
            "/test/v2",
            new_config
        ).await.unwrap();
        
        assert_eq!(migration_result.vectors_migrated, 30);
        assert_eq!(migration_result.old_clusters, 3);
        assert_eq!(migration_result.new_clusters, 10);
        
        // Load new index and verify
        let loaded = persister.load_index("/test/v2").await.unwrap();
        assert_eq!(loaded.total_vectors(), 30);
        assert_eq!(loaded.config().n_clusters, 10);
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

fn create_large_trained_index(n_clusters: usize) -> IVFIndex {
    let config = IVFConfig {
        n_clusters,
        n_probe: 10,
        train_size: n_clusters * 10,
        max_iterations: 25,
        seed: Some(42),
    };
    
    let mut index = IVFIndex::new(config);
    
    // Generate training data
    let training_data: Vec<Vec<f32>> = (0..n_clusters * 10)
        .map(|i| {
            let angle = i as f32 * 2.0 * std::f32::consts::PI / (n_clusters * 10) as f32;
            vec![angle.cos() * 10.0, angle.sin() * 10.0]
        })
        .collect();
    
    index.train(&training_data).unwrap();
    index
}

fn train_simple_index(index: &mut IVFIndex) {
    let training_data = vec![
        vec![0.0, 0.0], vec![0.1, 0.1], vec![0.2, -0.1],
        vec![5.0, 5.0], vec![5.1, 4.9], vec![4.9, 5.1],
        vec![-5.0, -5.0], vec![-4.9, -5.1], vec![-5.1, -4.9],
    ];
    
    index.train(&training_data).unwrap();
}

use vector_db::ivf::persistence::{serialize_centroids, calculate_total_size};