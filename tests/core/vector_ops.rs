// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use proptest::prelude::*;
use vector_db::core::types::*;
use vector_db::core::vector_ops::*;

#[cfg(test)]
mod vector_operation_tests {
    use super::*;

    #[test]
    fn test_batch_similarity_calculation() {
        let query = Embedding::new(vec![1.0, 0.0, 0.0]);
        let vectors = vec![
            Embedding::new(vec![1.0, 0.0, 0.0]),
            Embedding::new(vec![0.0, 1.0, 0.0]),
            Embedding::new(vec![0.707, 0.707, 0.0]),
        ];

        let similarities = batch_cosine_similarity(&query, &vectors);

        assert_eq!(similarities.len(), 3);
        assert!((similarities[0] - 1.0).abs() < 1e-6);
        assert!((similarities[1] - 0.0).abs() < 1e-6);
        assert!((similarities[2] - 0.707).abs() < 0.01);
    }

    #[test]
    fn test_top_k_selection() {
        let scores = vec![0.1, 0.9, 0.5, 0.7, 0.3, 0.8];
        let indices = top_k_indices(&scores, 3);

        assert_eq!(indices, vec![1, 5, 3]); // Indices of 0.9, 0.8, 0.7
    }

    #[test]
    fn test_result_merging() {
        let results1 = vec![
            SearchResult {
                vector_id: VectorId::from_string("a"),
                distance: 0.1,
                metadata: None,
            },
            SearchResult {
                vector_id: VectorId::from_string("b"),
                distance: 0.3,
                metadata: None,
            },
        ];

        let results2 = vec![
            SearchResult {
                vector_id: VectorId::from_string("b"),
                distance: 0.2, // Better score for 'b'
                metadata: None,
            },
            SearchResult {
                vector_id: VectorId::from_string("c"),
                distance: 0.4,
                metadata: None,
            },
        ];

        let merged = merge_search_results(vec![results1, results2], 3);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].distance, 0.1); // 'a'
        assert_eq!(merged[1].distance, 0.2); // 'b' with better score
        assert_eq!(merged[2].distance, 0.4); // 'c'
    }

    #[cfg(feature = "simd")]
    #[test]
    fn test_simd_operations() {
        let a = vec![1.0f32; 256];
        let b = vec![1.0f32; 256];

        let dot_scalar = dot_product_scalar(&a, &b);
        let dot_simd = dot_product_simd(&a, &b);

        assert!((dot_scalar - dot_simd).abs() < 1e-4);
        assert_eq!(dot_scalar, 256.0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_cosine_similarity_properties(
            vec_a in prop::collection::vec(-1.0f32..1.0f32, 10..100),
            vec_b in prop::collection::vec(-1.0f32..1.0f32, 10..100)
        ) {
            // Make vectors same length
            let min_len = vec_a.len().min(vec_b.len());
            let a = Embedding::new(vec_a[..min_len].to_vec());
            let b = Embedding::new(vec_b[..min_len].to_vec());

            let similarity = a.cosine_similarity(&b);

            // Cosine similarity is bounded [-1, 1]
            prop_assert!(similarity >= -1.0 && similarity <= 1.0);

            // Self-similarity is 1 (if not zero vector)
            if a.magnitude() > 1e-6 {
                let self_sim = a.cosine_similarity(&a);
                prop_assert!((self_sim - 1.0).abs() < 1e-6);
            }
        }

        #[test]
        fn test_euclidean_distance_properties(
            vec_a in prop::collection::vec(-100.0f32..100.0f32, 10..100),
            vec_b in prop::collection::vec(-100.0f32..100.0f32, 10..100)
        ) {
            let min_len = vec_a.len().min(vec_b.len());
            let a = Embedding::new(vec_a[..min_len].to_vec());
            let b = Embedding::new(vec_b[..min_len].to_vec());

            let dist_ab = a.euclidean_distance(&b);
            let dist_ba = b.euclidean_distance(&a);

            // Distance is symmetric
            prop_assert!((dist_ab - dist_ba).abs() < 1e-6);

            // Distance is non-negative
            prop_assert!(dist_ab >= 0.0);

            // Self-distance is zero
            let self_dist = a.euclidean_distance(&a);
            prop_assert!(self_dist.abs() < 1e-6);
        }
    }
}
