// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// Integration tests for manifest version handling and upgrade paths
use vector_db::core::chunk::{Manifest, MANIFEST_VERSION};
use vector_db::core::storage::{MockS5Storage, S5Storage};
use vector_db::hybrid::{HybridConfig, HybridIndex, HybridPersister};

// ============================================================================
// Version 2 Manifest Tests (Current Version)
// ============================================================================

#[tokio::test]
async fn test_version_2_manifest_parsing() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Create and save an empty index with version 2
    let config = HybridConfig::default();
    let index = HybridIndex::new(config);

    persister.save_index_chunked(&index, "test/v2").await.expect("Failed to save");

    // Load manifest and verify version
    let manifest_data = storage.get("test/v2/manifest.json").await.expect("Failed to get manifest").unwrap();
    let manifest_json = String::from_utf8(manifest_data).unwrap();
    let manifest = Manifest::from_json(&manifest_json).expect("Failed to parse manifest");

    assert_eq!(manifest.version, 2, "Manifest should be version 2");
    assert_eq!(manifest.version, MANIFEST_VERSION, "Manifest version should match constant");
}

#[tokio::test]
async fn test_load_version_2_manifest() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Save a valid version 2 index
    let index = HybridIndex::new(HybridConfig::default());
    persister.save_index_chunked(&index, "test/v2_load").await.expect("Failed to save");

    // Load should succeed
    let loaded = persister.load_index_chunked("test/v2_load").await;
    assert!(loaded.is_ok(), "Loading version 2 manifest should succeed");
}

// ============================================================================
// Version Rejection Tests
// ============================================================================

#[tokio::test]
async fn test_reject_future_version() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Create manifest with future version (3)
    let future_manifest = serde_json::json!({
        "version": 3,
        "chunk_size": 10000,
        "total_vectors": 0,
        "chunks": [],
        "hnsw_structure": null,
        "ivf_structure": null
    });

    let manifest_json = serde_json::to_string(&future_manifest).unwrap();
    storage.put("test/future/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save future manifest");

    // Try to load - should fail
    let result = persister.load_index_chunked("test/future").await;

    assert!(result.is_err(), "Should reject future version");
    if let Err(err) = result {
        let err_msg = err.to_string();
        assert!(err_msg.contains("Incompatible version") || err_msg.contains("expected"));
    }
}

#[tokio::test]
async fn test_reject_version_100() {
    let storage = MockS5Storage::new();
    let persister = HybridPersister::new(storage.clone());

    // Create manifest with way future version
    let future_manifest = serde_json::json!({
        "version": 100,
        "chunk_size": 10000,
        "total_vectors": 0,
        "chunks": []
    });

    let manifest_json = serde_json::to_string(&future_manifest).unwrap();
    storage.put("test/v100/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save v100 manifest");

    // Try to load - should fail with version error
    let result = persister.load_index_chunked("test/v100").await;
    assert!(result.is_err(), "Should reject version 100");
}

// ============================================================================
// Backward Compatibility Tests
// ============================================================================

#[tokio::test]
async fn test_accept_version_1() {
    let storage = MockS5Storage::new();

    // Create a minimal valid version 1 manifest
    let v1_manifest = serde_json::json!({
        "version": 1,
        "chunk_size": 10000,
        "total_vectors": 0,
        "chunks": []
    });

    let manifest_json = serde_json::to_string(&v1_manifest).unwrap();
    storage.put("test/v1/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save v1 manifest");

    // Parsing v1 manifest should succeed (backward compatible)
    let manifest_data = storage.get("test/v1/manifest.json").await.unwrap().unwrap();
    let manifest_str = String::from_utf8(manifest_data).unwrap();
    let result = Manifest::from_json(&manifest_str);

    assert!(result.is_ok(), "Version 1 manifest should be backward compatible");
}

// ============================================================================
// Missing Fields Tests
// ============================================================================

#[tokio::test]
async fn test_missing_version_field() {
    let storage = MockS5Storage::new();

    // Manifest missing version field
    let invalid_manifest = serde_json::json!({
        "chunk_size": 10000,
        "total_vectors": 0,
        "chunks": []
    });

    let manifest_json = serde_json::to_string(&invalid_manifest).unwrap();
    storage.put("test/no_version/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save invalid manifest");

    let manifest_data = storage.get("test/no_version/manifest.json").await.unwrap().unwrap();
    let manifest_str = String::from_utf8(manifest_data).unwrap();
    let result = Manifest::from_json(&manifest_str);

    assert!(result.is_err(), "Should fail when version field is missing");
}

#[tokio::test]
async fn test_missing_chunks_field() {
    let storage = MockS5Storage::new();

    // Manifest missing chunks field
    let invalid_manifest = serde_json::json!({
        "version": 2,
        "chunk_size": 10000,
        "total_vectors": 0
    });

    let manifest_json = serde_json::to_string(&invalid_manifest).unwrap();
    storage.put("test/no_chunks/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save invalid manifest");

    let manifest_data = storage.get("test/no_chunks/manifest.json").await.unwrap().unwrap();
    let manifest_str = String::from_utf8(manifest_data).unwrap();
    let result = Manifest::from_json(&manifest_str);

    assert!(result.is_err(), "Should fail when chunks field is missing");
}

#[tokio::test]
async fn test_missing_chunk_size_field() {
    let storage = MockS5Storage::new();

    // Manifest missing chunk_size field
    let invalid_manifest = serde_json::json!({
        "version": 2,
        "total_vectors": 0,
        "chunks": []
    });

    let manifest_json = serde_json::to_string(&invalid_manifest).unwrap();
    storage.put("test/no_chunk_size/manifest.json", manifest_json.into_bytes())
        .await
        .expect("Failed to save invalid manifest");

    let manifest_data = storage.get("test/no_chunk_size/manifest.json").await.unwrap().unwrap();
    let manifest_str = String::from_utf8(manifest_data).unwrap();
    let result = Manifest::from_json(&manifest_str);

    assert!(result.is_err(), "Should fail when chunk_size field is missing");
}
