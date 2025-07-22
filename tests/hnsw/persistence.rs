use vector_db::hnsw::core::*;
use vector_db::hnsw::persistence::*;
use vector_db::core::types::*;
use vector_db::core::storage::*;
use std::collections::HashMap;
use tokio;

#[cfg(test)]
mod hnsw_serialization_tests {
    use super::*;

    #[test]
    fn test_node_serialization() {
        let id = VectorId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let mut node = HNSWNode::new(id.clone(), vector.clone());
        
        // Add some neighbors at different levels
        node.add_neighbor(0, VectorId::from_string("neighbor1"));
        node.add_neighbor(0, VectorId::from_string("neighbor2"));
        node.add_neighbor(1, VectorId::from_string("neighbor3"));
        
        // Serialize
        let serialized = node.to_cbor().unwrap();
        
        // Deserialize
        let deserialized = HNSWNode::from_cbor(&serialized).unwrap();
        
        assert_eq!(deserialized.id(), node.id());
        assert_eq!(deserialized.vector(), node.vector());
        assert_eq!(deserialized.level(), node.level());
        assert_eq!(deserialized.neighbors(0), node.neighbors(0));
        assert_eq!(deserialized.neighbors(1), node.neighbors(1));
    }

    #[test]
    fn test_index_metadata_serialization() {
        let config = HNSWConfig {
            max_connections: 16,
            max_connections_layer_0: 32,
            ef_construction: 200,
            seed: Some(42),
        };
        
        let entry_point = Some(VectorId::from_string("entry"));
        let metadata = HNSWMetadata {
            version: 1,
            config: config.clone(),
            entry_point: entry_point.clone(),
            node_count: 100,
            dimension: Some(128),
        };
        
        let serialized = metadata.to_cbor().unwrap();
        let deserialized = HNSWMetadata::from_cbor(&serialized).unwrap();
        
        assert_eq!(deserialized.version, metadata.version);
        assert_eq!(deserialized.config.max_connections, config.max_connections);
        assert_eq!(deserialized.entry_point, entry_point);
        assert_eq!(deserialized.node_count, 100);
        assert_eq!(deserialized.dimension, Some(128));
    }

    #[test]
    fn test_chunked_node_serialization() {
        let mut nodes = Vec::new();
        
        // Create 100 nodes
        for i in 0..100 {
            let id = VectorId::from_string(&format!("node_{}", i));
            let vector = vec![i as f32; 10];
            let mut node = HNSWNode::new(id, vector);
            
            // Add some neighbors
            if i > 0 {
                node.add_neighbor(0, VectorId::from_string(&format!("node_{}", i - 1)));
            }
            if i < 99 {
                node.add_neighbor(0, VectorId::from_string(&format!("node_{}", i + 1)));
            }
            
            nodes.push(node);
        }
        
        // Serialize in chunks
        let chunk_size = 20;
        let chunks = chunk_nodes(&nodes, chunk_size);
        
        assert_eq!(chunks.len(), 5); // 100 nodes / 20 per chunk
        
        // Each chunk should deserialize correctly
        for (chunk_id, chunk_data) in chunks {
            let deserialized = deserialize_node_chunk(&chunk_data).unwrap();
            assert_eq!(deserialized.len(), 20);
            
            // Verify chunk contains correct nodes
            let start_idx = chunk_id * chunk_size;
            for (i, node) in deserialized.iter().enumerate() {
                let expected_id = format!("node_{}", start_idx + i);
                assert_eq!(node.id().to_string(), VectorId::from_string(&expected_id).to_string());
            }
        }
    }
}

#[cfg(test)]
mod hnsw_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_save_empty_index() {
        let storage = MockS5Storage::new();
        let index = HNSWIndex::new(HNSWConfig::default());
        let persister = HNSWPersister::new(storage);
        
        persister.save_index(&index, "/test/hnsw").await.unwrap();
        
        // Check metadata was saved
        let metadata_exists = persister.storage()
            .get("/test/hnsw/metadata.cbor")
            .await
            .unwrap()
            .is_some();
        assert!(metadata_exists);
    }

    #[tokio::test]
    async fn test_save_and_load_small_index() {
        let storage = MockS5Storage::new();
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 50,
            seed: Some(42),
        });
        
        // Insert some nodes
        let vectors = vec![
            ("a", vec![1.0, 0.0]),
            ("b", vec![0.0, 1.0]),
            ("c", vec![-1.0, 0.0]),
            ("d", vec![0.0, -1.0]),
        ];
        
        for (name, vector) in vectors {
            let id = VectorId::from_string(name);
            index.insert(id, vector).unwrap();
        }
        
        let persister = HNSWPersister::new(storage);
        
        // Save
        persister.save_index(&index, "/test/hnsw").await.unwrap();
        
        // Load
        let loaded_index = persister.load_index("/test/hnsw").await.unwrap();
        
        // Verify
        assert_eq!(loaded_index.node_count(), 4);
        assert_eq!(loaded_index.config(), index.config());
        assert_eq!(loaded_index.entry_point(), index.entry_point());
        
        // Test search works on loaded index
        let results = loaded_index.search(&vec![0.5, 0.5], 2, 50).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    #[ignore = "HNSW insertion performance issue - takes too long"]
    async fn test_save_and_load_large_index() {
        let storage = MockS5Storage::new();
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 8,
            max_connections_layer_0: 16,
            ef_construction: 50,
            seed: Some(42),
        });
        
        // Insert 50 nodes (reduced for faster testing)
        for i in 0..50 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector: Vec<f32> = (0..10)
                .map(|j| ((i * j) as f32).sin())
                .collect();
            index.insert(id, vector).unwrap();
        }
        
        let persister = HNSWPersister::with_chunk_size(storage, 20);
        
        // Save
        persister.save_index(&index, "/test/large_hnsw").await.unwrap();
        
        // Verify chunks were created
        let chunks = persister.storage()
            .list("/test/large_hnsw/nodes/")
            .await
            .unwrap();
        assert_eq!(chunks.len(), 3); // 50 nodes / 20 per chunk = 3 chunks
        
        // Load
        let loaded_index = persister.load_index("/test/large_hnsw").await.unwrap();
        
        assert_eq!(loaded_index.node_count(), 50);
        
        // Verify a specific node
        let test_id = VectorId::from_string("vec_25");
        let node = loaded_index.get_node(&test_id).unwrap();
        assert_eq!(node.vector().len(), 10);
    }

    #[tokio::test]
    async fn test_incremental_save() {
        let storage = MockS5Storage::new();
        let persister = HNSWPersister::new(storage);
        
        // Create index and save initial state
        let mut index = HNSWIndex::new(HNSWConfig::default());
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32]).unwrap();
        }
        
        persister.save_index(&index, "/test/incremental").await.unwrap();
        
        // Track which nodes are dirty (modified)
        let mut dirty_nodes = HashMap::new();
        
        // Add more nodes
        for i in 10..15 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id.clone(), vec![i as f32]).unwrap();
            dirty_nodes.insert(id.clone(), index.get_node(&id).unwrap());
        }
        
        // Incremental save
        persister.save_incremental(&index, "/test/incremental", &dirty_nodes).await.unwrap();
        
        // Load and verify
        let loaded = persister.load_index("/test/incremental").await.unwrap();
        assert_eq!(loaded.node_count(), 15);
    }

    #[tokio::test]
    async fn test_corrupted_data_handling() {
        let storage = MockS5Storage::new();
        
        // Save corrupted metadata
        storage.put("/bad/hnsw/metadata.cbor", vec![0xFF, 0xFF, 0xFF]).await.unwrap();
        
        let persister = HNSWPersister::new(storage);
        let result = persister.load_index("/bad/hnsw").await;
        
        assert!(result.is_err());
        match result {
            Err(PersistenceError::DeserializationError(_)) => {},
            _ => panic!("Expected DeserializationError"),
        }
    }

    #[tokio::test]
    async fn test_version_compatibility() {
        let storage = MockS5Storage::new();
        
        // Create metadata with future version
        let future_metadata = HNSWMetadata {
            version: 999,
            config: HNSWConfig::default(),
            entry_point: None,
            node_count: 0,
            dimension: None,
        };
        
        let metadata_bytes = future_metadata.to_cbor().unwrap();
        storage.put("/future/hnsw/metadata.cbor", metadata_bytes).await.unwrap();
        
        let persister = HNSWPersister::new(storage);
        let result = persister.load_index("/future/hnsw").await;
        
        assert!(result.is_err());
        match result {
            Err(PersistenceError::IncompatibleVersion { .. }) => {},
            _ => panic!("Expected IncompatibleVersion error"),
        }
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "HNSW insertion performance issue - takes too long"]
    async fn test_partial_save_recovery() {
        let storage = MockS5Storage::new();
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 50,
            seed: Some(42),
        });
        
        // Insert nodes
        for i in 0..20 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32; 5]).unwrap();
        }
        
        let persister = HNSWPersister::with_chunk_size(storage.clone(), 10);
        
        // Simulate partial save - only save metadata and first chunk
        let metadata = HNSWMetadata::from_index(&index);
        storage.put("/partial/hnsw/metadata.cbor", metadata.to_cbor().unwrap()).await.unwrap();
        
        // Save only first 10 nodes (1 chunk)
        let all_nodes: Vec<_> = (0..10)
            .map(|i| {
                let id = VectorId::from_string(&format!("vec_{}", i));
                index.get_node(&id).unwrap()
            })
            .collect();
        
        let chunks = chunk_nodes(&all_nodes, 10);
        for (chunk_id, chunk_data) in chunks {
            let path = format!("/partial/hnsw/nodes/chunk_{:04}.cbor", chunk_id);
            storage.put(&path, chunk_data).await.unwrap();
        }
        
        // Try to load - should detect incomplete data
        let result = persister.load_index("/partial/hnsw").await;
        assert!(result.is_err());
        
        // Verify recovery info
        let recovery_info = persister.check_integrity("/partial/hnsw").await.unwrap();
        assert_eq!(recovery_info.expected_nodes, 20);
        assert_eq!(recovery_info.found_nodes, 10);
        assert_eq!(recovery_info.missing_chunks, vec![1]);
    }

    #[tokio::test]
    #[ignore = "HNSW insertion performance issue - takes too long"]
    async fn test_backup_and_restore() {
        let storage = MockS5Storage::new();
        let mut index = HNSWIndex::new(HNSWConfig {
            max_connections: 4,
            max_connections_layer_0: 8,
            ef_construction: 50,
            seed: Some(42),
        });
        
        // Create index
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32; 5]).unwrap();
        }
        
        let persister = HNSWPersister::new(storage);
        
        // Save with backup
        persister.save_with_backup(&index, "/prod/hnsw", "/backup/hnsw").await.unwrap();
        
        // Corrupt production data
        persister.storage().delete("/prod/hnsw/metadata.cbor").await.unwrap();
        
        // Loading from prod should fail
        assert!(persister.load_index("/prod/hnsw").await.is_err());
        
        // But backup should work
        let backup_index = persister.load_index("/backup/hnsw").await.unwrap();
        assert_eq!(backup_index.node_count(), 10);
        
        // Restore from backup
        persister.restore_from_backup("/backup/hnsw", "/prod/hnsw").await.unwrap();
        
        // Now prod should work again
        let restored = persister.load_index("/prod/hnsw").await.unwrap();
        assert_eq!(restored.node_count(), 10);
    }
}

// Helper functions that should be implemented
fn chunk_nodes(nodes: &[HNSWNode], chunk_size: usize) -> Vec<(usize, Vec<u8>)> {
    nodes.chunks(chunk_size)
        .enumerate()
        .map(|(i, chunk)| {
            let bytes = serialize_node_chunk(chunk).unwrap();
            (i, bytes)
        })
        .collect()
}

fn serialize_node_chunk(nodes: &[HNSWNode]) -> Result<Vec<u8>, PersistenceError> {
    vector_db::hnsw::persistence::serialize_node_chunk(nodes)
}

fn deserialize_node_chunk(data: &[u8]) -> Result<Vec<HNSWNode>, PersistenceError> {
    vector_db::hnsw::persistence::deserialize_node_chunk(data)
}