// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

// tests/cbor_compatibility_tests.rs
// Tests to ensure CBOR encoding is deterministic and compatible with s5 patterns

use vector_db::cbor::{CborEncoder, CborDecoder};
use vector_db::types::{Vector, VideoNFTMetadata, Attribute, S5Metadata};
use hex;
use serde::{Serialize, Deserialize};
use serde_cbor::Value;

#[test]
fn test_vector_cbor_deterministic() {
    // Test that encoding is deterministic - same input always produces same output
    let vector = Vector {
        id: "test_vec_001".to_string(),
        values: vec![0.1_f32, 0.2_f32, 0.3_f32],
        metadata: Some(serde_json::json!({
            "source": "test",
            "timestamp": "2024-01-01T00:00:00Z"
        })),
    };
    
    // Encode multiple times
    let encoded1 = CborEncoder::encode_vector(&vector).unwrap();
    let encoded2 = CborEncoder::encode_vector(&vector).unwrap();
    
    // Should be identical
    assert_eq!(encoded1, encoded2, "Encoding should be deterministic");
    
    // Print for debugging
    println!("Vector CBOR hex: {}", hex::encode(&encoded1));
    
    // Should decode correctly
    let decoded = CborDecoder::decode_vector(&encoded1).unwrap();
    assert_eq!(vector.id, decoded.id);
    assert_eq!(vector.values, decoded.values);
}

#[test]
fn test_video_nft_metadata_cbor_encoding() {
    // Test encoding of actual video NFT metadata
    let metadata = VideoNFTMetadata {
        address: "0xFFbc1e2aFB6ED3d5C1ec98E87a2CB5d1e4aec2a6".to_string(),
        attributes: vec![
            Attribute {
                key: "release_date".to_string(),
                value: serde_json::json!("2022"),
            },
            Attribute {
                key: "runtime".to_string(),
                value: serde_json::json!(125),
            },
            Attribute {
                key: "languages".to_string(),
                value: serde_json::json!(["English"]),
            },
        ],
        description: "A woman who raised herself in the marshes...".to_string(),
        genre: vec!["Drama".to_string(), "Mystery".to_string()],
        id: "340".to_string(),
        image: "s5://uJh_FJwe3q8Da3NqX1s6end5GKic_uuwDSGk5plBMguNa2RaeDg.jpg".to_string(),
        mintDateTime: "2024-04-04T23:02:43.269Z".to_string(),
        name: "Where the Crawdads Sing".to_string(),
        posterImage: Some("s5://uJh_lUC7lpaMJvixPQtwfCKNbS3m5AXoPe22M6MzG9A6GkTqvCA.jpg".to_string()),
        summary: "A woman who raised herself in the marshes...".to_string(),
        supply: 1,
        symbol: "MV20".to_string(),
        r#type: "video".to_string(),
        uri: "ipfs://QmaNFjUuUksoBDJpYSm6hv6vkADp7CBzyhWxK6ucw1EUnG".to_string(),
        userPub: "QBg4r4ZzdI5DXtjUTBYol7HiW5EvU7H-Zv64CHei2YU.PHFbYSQ8zKaZYLCb7lD4BePUAs2fJzc171lpZs4zDj4".to_string(),
        video: "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI.mp4".to_string(),
    };
    
    // Encode
    let encoded = CborEncoder::encode_metadata(&metadata).unwrap();
    println!("Metadata CBOR length: {} bytes", encoded.len());
    
    // Decode and verify round-trip
    let decoded = CborDecoder::decode_metadata(&encoded).unwrap();
    assert_eq!(metadata.id, decoded.id);
    assert_eq!(metadata.name, decoded.name);
    assert_eq!(metadata.genre, decoded.genre);
    assert_eq!(metadata.address, decoded.address);
}

#[test]
fn test_cbor_empty_values() {
    // Test encoding of empty/minimal values (similar to s5-rs empty maps)
    let empty_vector = Vector {
        id: String::new(),
        values: vec![],
        metadata: None,
    };
    
    let encoded = CborEncoder::encode_vector(&empty_vector).unwrap();
    let decoded = CborDecoder::decode_vector(&encoded).unwrap();
    
    assert_eq!(empty_vector.id, decoded.id);
    assert_eq!(empty_vector.values, decoded.values);
    
    // Check that empty map encodes efficiently
    let cbor_value = serde_cbor::from_slice::<Value>(&encoded).unwrap();
    println!("Empty vector CBOR structure: {:?}", cbor_value);
}

#[test]
fn test_large_vector_cbor_encoding() {
    // Test 768-dimensional vector (typical embedding size)
    let large_vector = Vector {
        id: "large_vec_test".to_string(),
        values: vec![0.1_f32; 768], // 768 dimensions for all-mpnet-base-v2
        metadata: Some(serde_json::json!({
            "model": "all-mpnet-base-v2",
            "dimensions": 768
        })),
    };
    
    let encoded = CborEncoder::encode_vector(&large_vector).unwrap();
    
    // Verify encoding size is reasonable
    // 768 * 4 bytes (f32) + overhead should be ~3KB
    println!("Large vector CBOR size: {} bytes", encoded.len());
    assert!(encoded.len() < 4000, "Encoded size too large: {}", encoded.len());
    
    // Verify can decode
    let decoded = CborDecoder::decode_vector(&encoded).unwrap();
    assert_eq!(decoded.id, large_vector.id);
    assert_eq!(decoded.values.len(), 768);
    
    // Verify all values preserved
    for (orig, decoded) in large_vector.values.iter().zip(decoded.values.iter()) {
        assert!((orig - decoded).abs() < f32::EPSILON);
    }
}

#[test]
fn test_cbor_compression_compatibility() {
    // Test that compressed CBOR can be decoded
    let vector = Vector {
        id: "compress_test".to_string(),
        values: vec![0.5_f32; 384],
        metadata: None,
    };
    
    // Encode with compression
    let encoded = CborEncoder::encode_vector(&vector).unwrap();
    let compressed = CborEncoder::compress(&encoded).unwrap();
    
    println!("Original size: {} bytes", encoded.len());
    println!("Compressed size: {} bytes", compressed.len());
    
    // Verify compression actually reduces size
    assert!(compressed.len() < encoded.len());
    
    // Verify can decompress and decode
    let decompressed = CborDecoder::decompress(&compressed).unwrap();
    assert_eq!(encoded, decompressed);
    
    let decoded = CborDecoder::decode_vector(&decompressed).unwrap();
    assert_eq!(vector.id, decoded.id);
}

#[test]
fn test_s5_metadata_cbor_format() {
    // Test S5-specific metadata format
    let s5_meta = S5Metadata {
        cid: "s5://uJh-3y3T--m6C1BCS6_csHy5rijc7pl905qNNpBRvDFrmcVe1CAI".to_string(),
        size: 3456,
        mime_type: "application/cbor".to_string(),
        created_at: 1705745000, // Unix timestamp
        encryption: None,
    };
    
    let encoded = CborEncoder::encode_s5_metadata(&s5_meta).unwrap();
    let decoded = CborDecoder::decode_s5_metadata(&encoded).unwrap();
    
    assert_eq!(s5_meta.cid, decoded.cid);
    assert_eq!(s5_meta.size, decoded.size);
    assert_eq!(s5_meta.mime_type, decoded.mime_type);
}

#[test]
fn test_batch_cbor_encoding() {
    // Test batch encoding for efficiency
    let vectors: Vec<Vector> = (0..10)
        .map(|i| Vector {
            id: format!("vec_{}", i),
            values: vec![i as f32; 128],
            metadata: None,
        })
        .collect();
    
    // Encode batch
    let encoded_batch = CborEncoder::encode_batch(&vectors).unwrap();
    
    // Decode batch
    let decoded_batch = CborDecoder::decode_batch(&encoded_batch).unwrap();
    
    assert_eq!(vectors.len(), decoded_batch.len());
    for (original, decoded) in vectors.iter().zip(decoded_batch.iter()) {
        assert_eq!(original.id, decoded.id);
        assert_eq!(original.values, decoded.values);
    }
}

#[test]
#[ignore = "CBOR tags not fully supported by serde_cbor Tagged type"]
fn test_cbor_type_tags() {
    // Test CBOR tags for type identification
    let vector = Vector {
        id: "tagged_vec".to_string(),
        values: vec![1.0, 2.0],
        metadata: None,
    };
    
    // Encode with type tag (e.g., tag 42 for vectors)
    let encoded = CborEncoder::encode_with_tag(&vector, 42).unwrap();
    
    // Debug: print the encoded data
    println!("Tagged CBOR hex: {}", hex::encode(&encoded));
    
    // Try to decode as a Tagged value directly
    use serde_cbor::tags::Tagged;
    let tagged_result: Result<Tagged<Vector>, _> = serde_cbor::from_slice(&encoded);
    
    if let Ok(tagged) = tagged_result {
        println!("Decoded tag: {:?}", tagged.tag);
        assert_eq!(tagged.tag, Some(42));
        println!("Tag test passed!");
    } else {
        // If direct tagged decoding fails, try as Value
        let value = serde_cbor::from_slice::<Value>(&encoded).unwrap();
        println!("Decoded value: {:?}", value);
        
        if let Value::Tag(tag, _) = value {
            assert_eq!(tag, 42);
        } else {
            panic!("Expected tagged value, got: {:?}", value);
        }
    }
}

#[test]
fn test_special_float_values() {
    // Test handling of special float values
    let vector = Vector {
        id: "special_floats".to_string(),
        values: vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            std::f32::MIN,
            std::f32::MAX,
            std::f32::EPSILON,
        ],
        metadata: None,
    };
    
    let encoded = CborEncoder::encode_vector(&vector).unwrap();
    let decoded = CborDecoder::decode_vector(&encoded).unwrap();
    
    // Verify special values survive round-trip
    for (original, decoded) in vector.values.iter().zip(decoded.values.iter()) {
        if original.is_nan() && decoded.is_nan() {
            continue; // NaN != NaN, but both being NaN is ok
        }
        assert_eq!(original, decoded);
    }
}

#[test]
fn test_nft_attributes_encoding() {
    // Test encoding of attributes array
    let attributes = vec![
        Attribute {
            key: "release_date".to_string(),
            value: serde_json::json!("2022"),
        },
        Attribute {
            key: "runtime".to_string(),
            value: serde_json::json!(125),
        },
        Attribute {
            key: "languages".to_string(),
            value: serde_json::json!(["English", "Spanish"]),
        },
    ];
    
    let encoded = serde_cbor::to_vec(&attributes).unwrap();
    let decoded: Vec<Attribute> = serde_cbor::from_slice(&encoded).unwrap();
    
    assert_eq!(attributes.len(), decoded.len());
    
    // Check runtime value
    let runtime = decoded.iter()
        .find(|a| a.key == "runtime")
        .unwrap();
    assert_eq!(runtime.value.as_u64().unwrap(), 125);
}

#[test]
fn test_genre_array_encoding() {
    // Test genre array encoding
    let genres = vec!["Drama".to_string(), "Mystery".to_string(), "Romance".to_string()];
    
    let encoded = serde_cbor::to_vec(&genres).unwrap();
    let decoded: Vec<String> = serde_cbor::from_slice(&encoded).unwrap();
    
    assert_eq!(genres, decoded);
    
    // Check CBOR structure
    let value = serde_cbor::from_slice::<Value>(&encoded).unwrap();
    if let Value::Array(arr) = value {
        assert_eq!(arr.len(), 3);
    } else {
        panic!("Expected array");
    }
}

#[test]
fn test_nft_type_variations() {
    // Test different NFT types
    let types = vec!["video", "audio", "image", "data"];
    
    for nft_type in types {
        let metadata = VideoNFTMetadata {
            r#type: nft_type.to_string(),
            id: format!("test_{}", nft_type),
            name: format!("Test {} NFT", nft_type),
            address: "0x123".to_string(),
            attributes: vec![],
            description: "Test".to_string(),
            genre: vec![],
            image: "".to_string(),
            mintDateTime: "2024-01-01T00:00:00Z".to_string(),
            posterImage: None,
            summary: "Test".to_string(),
            supply: 1,
            symbol: "TEST".to_string(),
            uri: "".to_string(),
            userPub: "".to_string(),
            video: "".to_string(),
        };
        
        let encoded = CborEncoder::encode_metadata(&metadata).unwrap();
        let decoded = CborDecoder::decode_metadata(&encoded).unwrap();
        
        assert_eq!(decoded.r#type, nft_type);
    }
}

// Note: Mock types removed - these should come from the actual implementation