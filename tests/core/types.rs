// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use approx::assert_relative_eq;
use vector_db::core::types::*;

#[cfg(test)]
mod vector_id_tests {
    use super::*;

    #[test]
    fn test_vector_id_creation() {
        let id = VectorId::new();
        assert_eq!(id.as_bytes().len(), 32);
    }

    #[test]
    fn test_vector_id_from_string() {
        let id_str = "video_123_embedding";
        let id = VectorId::from_string(id_str);
        assert_eq!(id.to_string(), format!("vec_{}", &id.hash_hex()[..8]));
    }

    #[test]
    fn test_vector_id_serialization() {
        let id = VectorId::new();
        let serialized = id.to_cbor().unwrap();
        let deserialized = VectorId::from_cbor(&serialized).unwrap();
        assert_eq!(id, deserialized);
    }
}

#[cfg(test)]
mod embedding_tests {
    use super::*;

    #[test]
    fn test_embedding_creation() {
        let data = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let embedding = Embedding::new(data.clone());
        assert_eq!(embedding.dimension(), 5);
        assert_eq!(embedding.as_slice(), &data[..]);
    }

    #[test]
    fn test_embedding_normalization() {
        let data = vec![3.0, 4.0]; // 3-4-5 triangle
        let embedding = Embedding::new(data);
        let normalized = embedding.normalize();

        assert_relative_eq!(normalized.magnitude(), 1.0, epsilon = 1e-6);
        assert_relative_eq!(normalized.as_slice()[0], 0.6, epsilon = 1e-6);
        assert_relative_eq!(normalized.as_slice()[1], 0.8, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = Embedding::new(vec![1.0, 0.0, 0.0]);
        let b = Embedding::new(vec![0.0, 1.0, 0.0]);
        let c = Embedding::new(vec![1.0, 0.0, 0.0]);

        assert_relative_eq!(a.cosine_similarity(&b), 0.0, epsilon = 1e-6);
        assert_relative_eq!(a.cosine_similarity(&c), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = Embedding::new(vec![0.0, 0.0]);
        let b = Embedding::new(vec![3.0, 4.0]);

        assert_relative_eq!(a.euclidean_distance(&b), 5.0, epsilon = 1e-6);
    }

    #[test]
    #[should_panic(expected = "Dimension mismatch")]
    fn test_similarity_dimension_mismatch() {
        let a = Embedding::new(vec![1.0, 2.0]);
        let b = Embedding::new(vec![1.0, 2.0, 3.0]);
        a.cosine_similarity(&b);
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_video_metadata_creation() {
        let metadata = VideoMetadata {
            video_id: "video_123".to_string(),
            title: "Test Video".to_string(),
            description: Some("A test video".to_string()),
            tags: vec!["test".to_string(), "video".to_string()],
            duration_seconds: 120,
            upload_timestamp: Utc::now(),
            model_name: "all-MiniLM-L6-v2".to_string(),
            extra: Default::default(),
        };

        assert_eq!(metadata.video_id, "video_123");
        assert_eq!(metadata.tags.len(), 2);
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = VideoMetadata {
            video_id: "test".to_string(),
            title: "Test".to_string(),
            description: None,
            tags: vec![],
            duration_seconds: 60,
            upload_timestamp: Utc::now(),
            model_name: "model".to_string(),
            extra: Default::default(),
        };

        let serialized = metadata.to_cbor().unwrap();
        let deserialized = VideoMetadata::from_cbor(&serialized).unwrap();

        assert_eq!(metadata.video_id, deserialized.video_id);
        assert_eq!(metadata.title, deserialized.title);
    }
}

#[cfg(test)]
mod search_result_tests {
    use super::*;

    #[test]
    fn test_search_result_ordering() {
        let mut results = vec![
            SearchResult {
                vector_id: VectorId::from_string("a"),
                distance: 0.5,
                metadata: None,
            },
            SearchResult {
                vector_id: VectorId::from_string("b"),
                distance: 0.1,
                metadata: None,
            },
            SearchResult {
                vector_id: VectorId::from_string("c"),
                distance: 0.3,
                metadata: None,
            },
        ];

        results.sort();

        assert_eq!(results[0].distance, 0.1);
        assert_eq!(results[1].distance, 0.3);
        assert_eq!(results[2].distance, 0.5);
    }

    #[test]
    fn test_search_result_deduplication() {
        let id = VectorId::from_string("duplicate");
        let results = vec![
            SearchResult {
                vector_id: id.clone(),
                distance: 0.5,
                metadata: None,
            },
            SearchResult {
                vector_id: id.clone(),
                distance: 0.3, // Better score
                metadata: None,
            },
        ];

        let deduped = SearchResult::deduplicate(results);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].distance, 0.3); // Kept better score
    }
}
