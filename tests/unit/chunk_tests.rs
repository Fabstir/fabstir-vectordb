/// Unit tests for chunk types and manifest structures
use vector_db::core::chunk::{
    ChunkMetadata, HNSWManifest, IVFManifest, LayerMetadata, Manifest, VectorChunk,
};
use vector_db::core::types::VectorId;
use std::collections::HashMap;

// ============================================================================
// VectorChunk Tests
// ============================================================================

#[test]
fn test_vector_chunk_creation() {
    let chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    assert_eq!(chunk.chunk_id, "chunk-0");
    assert_eq!(chunk.start_idx, 0);
    assert_eq!(chunk.end_idx, 9999);
    assert_eq!(chunk.vectors.len(), 0);
}

#[test]
fn test_vector_chunk_add_vector() {
    let mut chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);
    let id = VectorId::from_string("vec1");
    let vector = vec![0.1, 0.2, 0.3, 0.4];

    chunk.add_vector(id.clone(), vector.clone());

    assert_eq!(chunk.vectors.len(), 1);
    assert_eq!(chunk.vectors.get(&id), Some(&vector));
}

#[test]
fn test_vector_chunk_cbor_serialization() {
    let mut chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    // Add some test vectors
    for i in 0..10 {
        let id = VectorId::from_string(&format!("vec{}", i));
        let vector = vec![i as f32 * 0.1; 4];
        chunk.add_vector(id, vector);
    }

    // Serialize to CBOR
    let cbor = chunk.to_cbor().expect("Failed to serialize");

    // Deserialize back
    let deserialized = VectorChunk::from_cbor(&cbor).expect("Failed to deserialize");

    // Verify
    assert_eq!(deserialized.chunk_id, chunk.chunk_id);
    assert_eq!(deserialized.start_idx, chunk.start_idx);
    assert_eq!(deserialized.end_idx, chunk.end_idx);
    assert_eq!(deserialized.vectors.len(), chunk.vectors.len());
}

#[test]
fn test_vector_chunk_empty_cbor_serialization() {
    let chunk = VectorChunk::new("chunk-empty".to_string(), 0, 0);

    // Serialize empty chunk
    let cbor = chunk.to_cbor().expect("Failed to serialize empty chunk");
    let deserialized = VectorChunk::from_cbor(&cbor).expect("Failed to deserialize");

    assert_eq!(deserialized.vectors.len(), 0);
}

// ============================================================================
// ChunkMetadata Tests
// ============================================================================

#[test]
fn test_chunk_metadata_creation() {
    let metadata = ChunkMetadata::new(
        "chunk-0".to_string(),
        1000,
        1024 * 1024, // 1 MB
        VectorId::from_string("vec0"),
        VectorId::from_string("vec999"),
    );

    assert_eq!(metadata.chunk_id, "chunk-0");
    assert_eq!(metadata.vector_count, 1000);
    assert_eq!(metadata.byte_size, 1024 * 1024);
    assert_eq!(metadata.cid, None);
}

#[test]
fn test_chunk_metadata_cbor_roundtrip() {
    let mut metadata = ChunkMetadata::new(
        "chunk-0".to_string(),
        1000,
        1024 * 1024,
        VectorId::from_string("vec0"),
        VectorId::from_string("vec999"),
    );
    metadata.cid = Some("z5AanNVJCxnN8kzXSKZuotYxgEcjw9".to_string());

    // Serialize to CBOR
    let cbor = metadata.to_cbor().expect("Failed to serialize");

    // Deserialize back
    let deserialized = ChunkMetadata::from_cbor(&cbor).expect("Failed to deserialize");

    assert_eq!(deserialized.chunk_id, metadata.chunk_id);
    assert_eq!(deserialized.vector_count, metadata.vector_count);
    assert_eq!(deserialized.byte_size, metadata.byte_size);
    assert_eq!(deserialized.cid, metadata.cid);
}

// ============================================================================
// Manifest Tests
// ============================================================================

#[test]
fn test_manifest_creation() {
    let manifest = Manifest::new(10000, 0);

    assert_eq!(manifest.version, 2);
    assert_eq!(manifest.chunk_size, 10000);
    assert_eq!(manifest.total_vectors, 0);
    assert_eq!(manifest.chunks.len(), 0);
    assert!(manifest.hnsw_structure.is_none());
    assert!(manifest.ivf_structure.is_none());
}

#[test]
fn test_manifest_add_chunk() {
    let mut manifest = Manifest::new(10000, 0);

    let metadata = ChunkMetadata::new(
        "chunk-0".to_string(),
        5000,
        1024 * 1024,
        VectorId::from_string("vec0"),
        VectorId::from_string("vec4999"),
    );

    manifest.add_chunk(metadata);

    assert_eq!(manifest.chunks.len(), 1);
    assert_eq!(manifest.chunks[0].chunk_id, "chunk-0");
}

#[test]
fn test_manifest_json_serialization() {
    let mut manifest = Manifest::new(10000, 25000);

    // Add 3 chunks
    for i in 0..3 {
        let metadata = ChunkMetadata::new(
            format!("chunk-{}", i),
            10000,
            15 * 1024 * 1024, // 15 MB
            VectorId::from_string(&format!("vec{}", i * 10000)),
            VectorId::from_string(&format!("vec{}", (i + 1) * 10000 - 1)),
        );
        manifest.add_chunk(metadata);
    }

    // Serialize to JSON
    let json = manifest.to_json().expect("Failed to serialize to JSON");

    // Deserialize back
    let deserialized = Manifest::from_json(&json).expect("Failed to deserialize from JSON");

    assert_eq!(deserialized.version, manifest.version);
    assert_eq!(deserialized.chunk_size, manifest.chunk_size);
    assert_eq!(deserialized.total_vectors, manifest.total_vectors);
    assert_eq!(deserialized.chunks.len(), manifest.chunks.len());
}

#[test]
fn test_manifest_version_validation() {
    // Test that future versions are rejected
    let json = r#"{"version":999,"chunk_size":10000,"total_vectors":0,"chunks":[]}"#;

    let result = Manifest::from_json(json);

    assert!(result.is_err());

    // Test that current and older versions are accepted (backward compatibility)
    let json_v2 = r#"{"version":2,"chunk_size":10000,"total_vectors":0,"chunks":[]}"#;
    let result_v2 = Manifest::from_json(json_v2);
    assert!(result_v2.is_ok());

    let json_v1 = r#"{"version":1,"chunk_size":10000,"total_vectors":0,"chunks":[]}"#;
    let result_v1 = Manifest::from_json(json_v1);
    assert!(result_v1.is_ok());
}

#[test]
fn test_manifest_with_hnsw_structure() {
    let mut manifest = Manifest::new(10000, 10000);

    let hnsw_manifest = HNSWManifest {
        entry_point: VectorId::from_string("vec0"),
        layers: vec![
            LayerMetadata {
                layer_id: 0,
                node_count: 10000,
            },
            LayerMetadata {
                layer_id: 1,
                node_count: 1000,
            },
        ],
        node_chunk_map: {
            let mut map = HashMap::new();
            map.insert(VectorId::from_string("vec0").to_string(), "chunk-0".to_string());
            map.insert(VectorId::from_string("vec5000").to_string(), "chunk-0".to_string());
            map
        },
    };

    manifest.hnsw_structure = Some(hnsw_manifest);

    // Serialize and deserialize
    let json = manifest.to_json().expect("Failed to serialize");
    let deserialized = Manifest::from_json(&json).expect("Failed to deserialize");

    assert!(deserialized.hnsw_structure.is_some());
    let hnsw = deserialized.hnsw_structure.unwrap();
    assert_eq!(hnsw.layers.len(), 2);
    assert_eq!(hnsw.node_chunk_map.len(), 2);
}

#[test]
fn test_manifest_with_ivf_structure() {
    let mut manifest = Manifest::new(10000, 10000);

    let ivf_manifest = IVFManifest {
        centroids: vec![
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.5, 0.6, 0.7, 0.8],
        ],
        cluster_assignments: {
            let mut map = HashMap::new();
            map.insert(0, vec!["chunk-0".to_string(), "chunk-1".to_string()]);
            map.insert(1, vec!["chunk-1".to_string(), "chunk-2".to_string()]);
            map
        },
    };

    manifest.ivf_structure = Some(ivf_manifest);

    // Serialize and deserialize
    let json = manifest.to_json().expect("Failed to serialize");
    let deserialized = Manifest::from_json(&json).expect("Failed to deserialize");

    assert!(deserialized.ivf_structure.is_some());
    let ivf = deserialized.ivf_structure.unwrap();
    assert_eq!(ivf.centroids.len(), 2);
    assert_eq!(ivf.cluster_assignments.len(), 2);
}

// ============================================================================
// Validation Tests
// ============================================================================

#[test]
fn test_chunk_range_no_overlap() {
    let chunk1 = VectorChunk::new("chunk-0".to_string(), 0, 9999);
    let chunk2 = VectorChunk::new("chunk-1".to_string(), 10000, 19999);

    assert!(!chunk1.overlaps_with(&chunk2));
    assert!(!chunk2.overlaps_with(&chunk1));
}

#[test]
fn test_chunk_range_with_overlap() {
    let chunk1 = VectorChunk::new("chunk-0".to_string(), 0, 10000);
    let chunk2 = VectorChunk::new("chunk-1".to_string(), 5000, 15000);

    assert!(chunk1.overlaps_with(&chunk2));
    assert!(chunk2.overlaps_with(&chunk1));
}

#[test]
fn test_manifest_validate_no_overlaps() {
    let mut manifest = Manifest::new(10000, 30000);

    // Add non-overlapping chunks
    for i in 0..3 {
        let metadata = ChunkMetadata::new(
            format!("chunk-{}", i),
            10000,
            15 * 1024 * 1024,
            VectorId::from_string(&format!("vec{}", i * 10000)),
            VectorId::from_string(&format!("vec{}", (i + 1) * 10000 - 1)),
        );
        manifest.add_chunk(metadata);
    }

    // Validation should pass
    assert!(manifest.validate().is_ok());
}

// ============================================================================
// Property-Based Tests (using simple randomization)
// ============================================================================

#[test]
fn test_chunk_partitioning_coverage() {
    // Test that chunks cover all vectors without gaps
    let total_vectors = 25000;
    let chunk_size = 10000;
    let expected_chunks = 3; // ceil(25000 / 10000)

    let mut manifest = Manifest::new(chunk_size, total_vectors);

    // Simulate chunk creation
    let mut offset = 0;
    while offset < total_vectors {
        let end = std::cmp::min(offset + chunk_size, total_vectors);
        let count = end - offset;

        let metadata = ChunkMetadata::new(
            format!("chunk-{}", manifest.chunks.len()),
            count,
            count * 384 * 4, // Estimate: 384D float32
            VectorId::from_string(&format!("vec{}", offset)),
            VectorId::from_string(&format!("vec{}", end - 1)),
        );

        manifest.add_chunk(metadata);
        offset = end;
    }

    assert_eq!(manifest.chunks.len(), expected_chunks);

    // Verify total coverage
    let total_covered: usize = manifest.chunks.iter().map(|c| c.vector_count).sum();
    assert_eq!(total_covered, total_vectors);
}

#[test]
fn test_large_chunk_serialization() {
    // Test with a chunk containing many vectors
    let mut chunk = VectorChunk::new("chunk-large".to_string(), 0, 9999);

    // Add 1000 vectors
    for i in 0..1000 {
        let id = VectorId::from_string(&format!("vec{}", i));
        let vector = vec![i as f32 * 0.01; 384]; // 384D vector
        chunk.add_vector(id, vector);
    }

    // Serialize and deserialize
    let cbor = chunk.to_cbor().expect("Failed to serialize large chunk");
    let deserialized = VectorChunk::from_cbor(&cbor).expect("Failed to deserialize large chunk");

    assert_eq!(deserialized.vectors.len(), 1000);

    // Verify a sample vector
    let sample_id = VectorId::from_string("vec500");
    let original_vec = chunk.vectors.get(&sample_id).unwrap();
    let deserialized_vec = deserialized.vectors.get(&sample_id).unwrap();

    assert_eq!(original_vec, deserialized_vec);
}
