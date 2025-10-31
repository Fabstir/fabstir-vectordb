// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests for metadata update persistence through save/load operations
//!
//! These tests verify that metadata updates made via the Node.js bindings
//! (or similar interfaces) properly persist when the index is saved to S5
//! and loaded back.

use std::collections::HashMap;
use vector_db::{
    core::{
        storage::MockS5Storage,
        types::VectorId,
    },
    hybrid::{HybridConfig, HybridIndex, HybridPersister},
};

/// Helper to create a test hybrid index with training data
async fn create_test_index() -> HybridIndex {
    let config = HybridConfig::default();
    let mut index = HybridIndex::new(config);

    // Initialize with training data (10 vectors for IVF)
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

/// Helper to add test vectors with metadata
async fn add_test_vectors_with_metadata(
    index: &mut HybridIndex,
    metadata_map: &mut HashMap<String, serde_json::Value>,
    count: usize,
) {
    for i in 0..count {
        let id = format!("vec-{}", i);
        let vector_id = VectorId::from_string(&id);
        let vector: Vec<f32> = (0..128)
            .map(|j| ((i + j) as f32).sin() * 0.5)
            .collect();

        // Add vector to index
        index.insert(vector_id.clone(), vector).await.unwrap();

        // Add metadata
        let metadata = serde_json::json!({
            "_originalId": id,
            "index": i,
            "status": "initial",
            "timestamp": 1000 + i
        });

        metadata_map.insert(vector_id.to_string(), metadata);
    }
}

#[tokio::test]
async fn test_metadata_updates_persist_after_save_load() {
    // Create index and metadata
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();

    // Add vectors with initial metadata
    add_test_vectors_with_metadata(&mut index, &mut metadata_map, 5).await;

    // Update metadata for some vectors (simulating updateMetadata operation)
    let vec_1 = VectorId::from_string("vec-1");
    let vec_3 = VectorId::from_string("vec-3");

    metadata_map.insert(
        vec_1.to_string(),
        serde_json::json!({
            "_originalId": "vec-1",
            "index": 1,
            "status": "updated",
            "timestamp": 2000,
            "extra": "new field"
        }),
    );

    metadata_map.insert(
        vec_3.to_string(),
        serde_json::json!({
            "_originalId": "vec-3",
            "index": 3,
            "status": "modified",
            "timestamp": 3000,
        }),
    );

    // Save to mock S5
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    persister
        .save_index_chunked(&index, "test-metadata-persist")
        .await
        .unwrap();

    // Save metadata map to S5 (simulating session.rs save_to_s5)
    let metadata_cbor = serde_cbor::to_vec(&metadata_map).unwrap();
    storage
        .put("test-metadata-persist/metadata_map.cbor", metadata_cbor)
        .await
        .unwrap();

    // Load from S5 in new index
    let config = HybridConfig::default();
    let loaded_index = persister
        .load_index_chunked("test-metadata-persist", config)
        .await
        .unwrap();

    // Load metadata map
    let loaded_metadata_cbor = storage
        .get("test-metadata-persist/metadata_map.cbor")
        .await
        .unwrap()
        .expect("Metadata should exist");

    let loaded_metadata_map: HashMap<String, serde_json::Value> =
        serde_cbor::from_slice(&loaded_metadata_cbor).unwrap();

    // Verify loaded metadata contains updates
    let vec_1_meta = loaded_metadata_map.get(&vec_1.to_string()).unwrap();
    assert_eq!(vec_1_meta["status"], "updated");
    assert_eq!(vec_1_meta["extra"], "new field");
    assert_eq!(vec_1_meta["timestamp"], 2000);

    let vec_3_meta = loaded_metadata_map.get(&vec_3.to_string()).unwrap();
    assert_eq!(vec_3_meta["status"], "modified");
    assert_eq!(vec_3_meta["timestamp"], 3000);

    // Verify unchanged vectors have original metadata
    let vec_0 = VectorId::from_string("vec-0");
    let vec_0_meta = loaded_metadata_map.get(&vec_0.to_string()).unwrap();
    assert_eq!(vec_0_meta["status"], "initial");
    assert_eq!(vec_0_meta["timestamp"], 1000);

    // Verify index still contains all vectors
    assert_eq!(loaded_index.active_count().await, 5);
}

#[tokio::test]
async fn test_updated_metadata_returned_in_search_after_reload() {
    // Create index and metadata
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();

    // Add vectors with initial metadata
    add_test_vectors_with_metadata(&mut index, &mut metadata_map, 3).await;

    // Update metadata for vec-1
    let vec_1 = VectorId::from_string("vec-1");
    metadata_map.insert(
        vec_1.to_string(),
        serde_json::json!({
            "_originalId": "vec-1",
            "index": 1,
            "category": "updated-category",
            "priority": "high"
        }),
    );

    // Save and load
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    persister
        .save_index_chunked(&index, "test-search-metadata")
        .await
        .unwrap();

    let metadata_cbor = serde_cbor::to_vec(&metadata_map).unwrap();
    storage
        .put("test-search-metadata/metadata_map.cbor", metadata_cbor)
        .await
        .unwrap();

    // Load
    let config = HybridConfig::default();
    let loaded_index = persister
        .load_index_chunked("test-search-metadata", config)
        .await
        .unwrap();

    let loaded_metadata_cbor = storage
        .get("test-search-metadata/metadata_map.cbor")
        .await
        .unwrap()
        .unwrap();

    let loaded_metadata_map: HashMap<String, serde_json::Value> =
        serde_cbor::from_slice(&loaded_metadata_cbor).unwrap();

    // Search for vec-1
    let query: Vec<f32> = (0..128).map(|j| ((1 + j) as f32).sin() * 0.5).collect();
    let results = loaded_index.search(&query, 3).await.unwrap();

    // Find vec-1 in results
    let vec_1_result = results
        .iter()
        .find(|r| r.vector_id.to_string() == vec_1.to_string())
        .expect("Should find vec-1 in results");

    // Verify updated metadata can be retrieved
    let result_metadata = loaded_metadata_map
        .get(&vec_1_result.vector_id.to_string())
        .unwrap();

    assert_eq!(result_metadata["category"], "updated-category");
    assert_eq!(result_metadata["priority"], "high");
    assert_eq!(result_metadata["_originalId"], "vec-1");
}

#[tokio::test]
async fn test_update_save_load_search_roundtrip() {
    // Create index and metadata
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();

    // Add vectors
    add_test_vectors_with_metadata(&mut index, &mut metadata_map, 4).await;

    // Step 1: Update metadata
    let vec_2 = VectorId::from_string("vec-2");
    metadata_map.insert(
        vec_2.to_string(),
        serde_json::json!({
            "_originalId": "vec-2",
            "step": "updated",
            "data": "modified value"
        }),
    );

    // Step 2: Save
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    persister
        .save_index_chunked(&index, "test-roundtrip")
        .await
        .unwrap();

    let metadata_cbor = serde_cbor::to_vec(&metadata_map).unwrap();
    storage
        .put("test-roundtrip/metadata_map.cbor", metadata_cbor)
        .await
        .unwrap();

    // Step 3: Load
    let config = HybridConfig::default();
    let loaded_index = persister
        .load_index_chunked("test-roundtrip", config)
        .await
        .unwrap();

    let loaded_metadata_cbor = storage
        .get("test-roundtrip/metadata_map.cbor")
        .await
        .unwrap()
        .unwrap();

    let loaded_metadata_map: HashMap<String, serde_json::Value> =
        serde_cbor::from_slice(&loaded_metadata_cbor).unwrap();

    // Step 4: Search and verify
    let query: Vec<f32> = (0..128).map(|j| ((2 + j) as f32).sin() * 0.5).collect();
    let results = loaded_index.search(&query, 1).await.unwrap();

    assert_eq!(results.len(), 1);
    let result = &results[0];

    let result_metadata = loaded_metadata_map.get(&result.vector_id.to_string()).unwrap();

    assert_eq!(result_metadata["step"], "updated");
    assert_eq!(result_metadata["data"], "modified value");
    assert_eq!(result_metadata["_originalId"], "vec-2");
}

#[tokio::test]
async fn test_metadata_saved_to_s5_correctly() {
    // Create index and metadata
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();

    // Add vectors with complex metadata
    for i in 0..3 {
        let id = format!("complex-{}", i);
        let vector_id = VectorId::from_string(&id);
        let vector: Vec<f32> = (0..128).map(|j| ((i + j) as f32).sin() * 0.5).collect();

        index.insert(vector_id.clone(), vector).await.unwrap();

        metadata_map.insert(
            vector_id.to_string(),
            serde_json::json!({
                "_originalId": id,
                "nested": {
                    "deep": {
                        "value": format!("data-{}", i)
                    }
                },
                "array": [1, 2, 3],
                "bool": true,
                "null": null
            }),
        );
    }

    // Save to S5
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    persister
        .save_index_chunked(&index, "test-complex-metadata")
        .await
        .unwrap();

    let metadata_cbor = serde_cbor::to_vec(&metadata_map).unwrap();
    storage
        .put("test-complex-metadata/metadata_map.cbor", metadata_cbor)
        .await
        .unwrap();

    // Verify S5 storage contains metadata
    let stored_metadata_cbor = storage
        .get("test-complex-metadata/metadata_map.cbor")
        .await
        .unwrap()
        .expect("Metadata should be stored in S5");

    // Deserialize and verify structure
    let stored_metadata_map: HashMap<String, serde_json::Value> =
        serde_cbor::from_slice(&stored_metadata_cbor).unwrap();

    assert_eq!(stored_metadata_map.len(), 3);

    // Verify complex nested structures preserved
    let vec_0 = VectorId::from_string("complex-0");
    let vec_0_meta = stored_metadata_map.get(&vec_0.to_string()).unwrap();

    assert_eq!(vec_0_meta["nested"]["deep"]["value"], "data-0");
    assert_eq!(vec_0_meta["array"], serde_json::json!([1, 2, 3]));
    assert_eq!(vec_0_meta["bool"], true);
    assert_eq!(vec_0_meta["null"], serde_json::Value::Null);
}
