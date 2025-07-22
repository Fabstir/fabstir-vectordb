use criterion::{black_box, Criterion};
use std::time::Duration;
use vector_db::core::types::*;
use vector_db::core::vector_ops::*;

#[cfg(test)]
mod simd_tests {
    use super::*;

    #[test]
    fn test_dot_product_accuracy() {
        let sizes = vec![16, 64, 128, 256, 512, 1024];

        for size in sizes {
            let a: Vec<f32> = (0..size).map(|i| (i as f32).sin()).collect();
            let b: Vec<f32> = (0..size).map(|i| (i as f32).cos()).collect();

            let scalar_result = dot_product_scalar(&a, &b);
            let simd_result = dot_product_simd(&a, &b);

            // SIMD should be accurate within floating point tolerance
            assert!(
                (scalar_result - simd_result).abs() < 1e-4,
                "Size {}: scalar={}, simd={}",
                size,
                scalar_result,
                simd_result
            );
        }
    }

    #[test]
    fn test_cosine_similarity_simd() {
        let a = vec![1.0f32; 256];
        let b = vec![1.0f32; 256];

        let scalar_sim = cosine_similarity_scalar(&a, &b);
        let simd_sim = cosine_similarity_simd(&a, &b);

        assert!((scalar_sim - simd_sim).abs() < 1e-6);
        assert!((simd_sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance_simd() {
        let a = vec![0.0f32; 128];
        let b = vec![1.0f32; 128];

        let scalar_dist = euclidean_distance_scalar(&a, &b);
        let simd_dist = euclidean_distance_simd(&a, &b);

        let expected = (128.0f32).sqrt();
        assert!((scalar_dist - expected).abs() < 1e-4);
        assert!((simd_dist - expected).abs() < 1e-4);
    }

    #[test]
    fn test_batch_normalize_simd() {
        let vectors = vec![
            vec![3.0, 4.0],  // magnitude 5
            vec![5.0, 12.0], // magnitude 13
            vec![8.0, 15.0], // magnitude 17
        ];

        let normalized = batch_normalize(&vectors);

        for (i, norm_vec) in normalized.iter().enumerate() {
            let magnitude = dot_product_scalar(norm_vec, norm_vec).sqrt();
            assert!(
                (magnitude - 1.0).abs() < 1e-6,
                "Vector {} magnitude: {}",
                i,
                magnitude
            );
        }
    }
}

#[cfg(test)]
mod heap_tests {
    use super::*;

    #[test]
    fn test_top_k_heap_implementation() {
        let scores = vec![0.9, 0.1, 0.7, 0.3, 0.8, 0.2, 0.6, 0.4, 0.5];

        // Test various k values
        for k in 1..=scores.len() {
            let indices = top_k_indices_heap(&scores, k);
            assert_eq!(indices.len(), k);

            // Verify ordering (highest scores first)
            for i in 1..indices.len() {
                assert!(scores[indices[i - 1]] >= scores[indices[i]]);
            }
        }
    }

    #[test]
    fn test_streaming_top_k() {
        // Simulate streaming similarity scores
        let mut top_k = StreamingTopK::new(3);

        let items = vec![
            (VectorId::from_string("a"), 0.5),
            (VectorId::from_string("b"), 0.9),
            (VectorId::from_string("c"), 0.3),
            (VectorId::from_string("d"), 0.7),
            (VectorId::from_string("e"), 0.8),
        ];

        for (id, score) in items {
            top_k.add(id, score);
        }

        let results = top_k.get_results();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].distance, 0.9); // 'b'
        assert_eq!(results[1].distance, 0.8); // 'e'
        assert_eq!(results[2].distance, 0.7); // 'd'
    }
}

#[cfg(test)]
mod parallel_tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_parallel_similarity_computation() {
        let query = Embedding::new(vec![1.0; 512]);
        let vectors: Vec<Embedding> = (0..1000)
            .map(|i| {
                let v: Vec<f32> = (0..512).map(|j| ((i * j) as f32).sin()).collect();
                Embedding::new(v)
            })
            .collect();

        let similarities = compute_similarities_parallel(&query, &vectors, 4).await;

        assert_eq!(similarities.len(), 1000);

        // Verify some results
        for (i, sim) in similarities.iter().enumerate().take(10) {
            let expected = query.cosine_similarity(&vectors[i]);
            assert!((sim - expected).abs() < 1e-6);
        }
    }

    #[tokio::test]
    async fn test_parallel_batch_processing() {
        let queries = vec![
            Embedding::new(vec![1.0, 0.0, 0.0]),
            Embedding::new(vec![0.0, 1.0, 0.0]),
            Embedding::new(vec![0.0, 0.0, 1.0]),
        ];

        let vectors = vec![
            Embedding::new(vec![1.0, 0.0, 0.0]),
            Embedding::new(vec![0.0, 1.0, 0.0]),
            Embedding::new(vec![0.0, 0.0, 1.0]),
        ];

        let results = batch_search_parallel(&queries, &vectors, 2).await;

        assert_eq!(results.len(), 3);
        for (i, query_results) in results.iter().enumerate() {
            assert_eq!(query_results.len(), 2);
            // Each query should find itself as the best match
            assert_eq!(query_results[0].distance, 1.0);
        }
    }
}

#[cfg(test)]
mod quantization_tests {
    use super::*;

    #[test]
    fn test_scalar_quantization() {
        let vector = vec![0.0, 0.25, 0.5, 0.75, 1.0];

        // Quantize to 8 bits
        let quantized = scalar_quantize_u8(&vector);
        assert_eq!(quantized.data.len(), 5);
        assert_eq!(quantized.data[0], 0);
        assert_eq!(quantized.data[4], 255);

        // Dequantize
        let dequantized = quantized.dequantize();
        for (orig, deq) in vector.iter().zip(dequantized.iter()) {
            assert!((orig - deq).abs() < 0.01); // Within quantization error
        }
    }

    #[test]
    fn test_product_quantization() {
        let vectors: Vec<Vec<f32>> = (0..100)
            .map(|i| (0..128).map(|j| ((i * j) as f32).sin()).collect())
            .collect();

        // Train PQ with 16 subspaces, 256 centroids each
        let mut pq = ProductQuantizer::new(16, 256);
        pq.train(&vectors, 10); // 10 iterations

        // Encode vectors
        let encoded: Vec<_> = vectors.iter().map(|v| pq.encode(v)).collect();

        // Check compression ratio
        let original_size = vectors.len() * 128 * 4; // f32 bytes
        let compressed_size = encoded.len() * 16; // u8 per subspace
        let ratio = original_size as f32 / compressed_size as f32;
        assert!(ratio > 30.0); // Should achieve >30x compression

        // Test reconstruction error
        let mut max_error = 0.0;
        for (i, (orig, enc)) in vectors.iter().zip(encoded.iter()).enumerate() {
            let reconstructed = pq.decode(enc);
            let error = euclidean_distance_scalar(orig, &reconstructed);
            let norm = euclidean_distance_scalar(orig, &vec![0.0; 128]);
            let relative_error = if norm > 0.0 { error / norm } else { 0.0 };

            if relative_error > max_error {
                max_error = relative_error;
            }

            if i < 5 || relative_error >= 0.2 {
                println!("Vector {}: orig_len={}, recon_len={}, error={:.4}, norm={:.4}, relative_error={:.4}", 
                    i, orig.len(), reconstructed.len(), error, norm, relative_error);
            }

            assert!(
                relative_error < 0.2,
                "Vector {} has relative error {:.4} >= 0.2",
                i,
                relative_error
            ); // <20% relative error
        }
        println!("Max relative error: {:.4}", max_error);
    }
}

#[cfg(test)]
mod distance_correction_tests {
    use super::*;

    #[test]
    fn test_inner_product_to_cosine() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];

        let inner_product = dot_product_scalar(&a, &b);
        let cosine = inner_product_to_cosine(inner_product, &a, &b);
        let expected = cosine_similarity_scalar(&a, &b);

        assert!((cosine - expected).abs() < 1e-6);
    }

    #[test]
    fn test_angular_distance() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let c = vec![-1.0, 0.0];

        let dist_ab = angular_distance(&a, &b);
        let dist_ac = angular_distance(&a, &c);

        // 90 degrees
        assert!((dist_ab - std::f32::consts::PI / 2.0).abs() < 1e-6);
        // 180 degrees
        assert!((dist_ac - std::f32::consts::PI).abs() < 1e-6);
    }
}

// Benchmark utilities for performance testing
pub fn create_vector_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_operations");
    group.measurement_time(Duration::from_secs(10));

    for size in &[128, 512, 1536] {
        let a = vec![1.0f32; *size];
        let b = vec![1.0f32; *size];

        group.bench_function(&format!("dot_product_scalar_{}", size), |bencher| {
            bencher.iter(|| dot_product_scalar(black_box(&a), black_box(&b)))
        });

        group.bench_function(&format!("dot_product_simd_{}", size), |bencher| {
            bencher.iter(|| dot_product_simd(black_box(&a), black_box(&b)))
        });
    }

    group.finish();
}
