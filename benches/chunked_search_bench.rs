// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// Performance benchmarks for chunked storage search operations
/// Measures cold cache, warm cache, chunk loading, and cache effectiveness
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::collections::HashMap;
use std::time::Duration;
use tokio::runtime::Runtime;
use chrono::Utc;

use vector_db::core::storage::MockS5Storage;
use vector_db::core::types::VectorId;
use vector_db::hybrid::{HybridConfig, HybridIndex, HybridPersister};
use vector_db::hnsw::core::HNSWNode;
use vector_db::ivf::core::{Centroid, ClusterId, InvertedList};

// ============================================================================
// Constants
// ============================================================================

const DIMENSIONS: usize = 384; // Standard embedding dimension
const BENCHMARK_VECTOR_COUNT: usize = 10_000; // Smaller dataset for benchmarks

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate test vectors with deterministic values
fn create_test_vectors(count: usize, dimensions: usize, seed: usize) -> Vec<(VectorId, Vec<f32>)> {
    (0..count)
        .map(|i| {
            let id = VectorId::from_string(&format!("bench-vec-{}-{}", seed, i));
            let base = ((i + seed) as f32 * 0.001) % 1.0;
            let vector: Vec<f32> = (0..dimensions)
                .map(|d| base + (d as f32 * 0.0001))
                .collect();
            (id, vector)
        })
        .collect()
}

/// Setup benchmark index (synchronous wrapper for async setup)
fn setup_benchmark_index(
    rt: &Runtime,
    vector_count: usize,
    dimensions: usize,
) -> (HybridIndex, Vec<Vec<f32>>) {
    rt.block_on(async {
        let config = HybridConfig::default();

        // Create HNSW index
        let mut hnsw_index = vector_db::hnsw::core::HNSWIndex::new(config.hnsw_config.clone());

        // Create IVF index with centroids
        let mut ivf_index = vector_db::ivf::core::IVFIndex::new(config.ivf_config.clone());
        let num_centroids = config.ivf_config.n_clusters.min(256);
        let centroids: Vec<Centroid> = (0..num_centroids)
            .map(|i| {
                let base = (i as f32 * 0.1) % 1.0;
                let vector = vec![base; dimensions];
                Centroid::new(ClusterId(i), vector)
            })
            .collect();
        ivf_index.set_trained(centroids, dimensions);

        // Generate vectors
        let test_vectors = create_test_vectors(vector_count, dimensions, 42);
        let mut vectors = Vec::new();
        let mut timestamps = HashMap::new();

        // Split: 30% HNSW (recent), 70% IVF (historical)
        let hnsw_count = (vector_count * 3 / 10).max(100);
        let ivf_count = vector_count - hnsw_count;

        // Add vectors to HNSW
        let mut first_hnsw_id: Option<VectorId> = None;
        for (id, vector) in test_vectors.iter().take(hnsw_count) {
            let node = HNSWNode::new(id.clone(), vector.clone());
            hnsw_index.restore_node(node).expect("Failed to restore HNSW node");

            if first_hnsw_id.is_none() {
                first_hnsw_id = Some(id.clone());
            }

            vectors.push(vector.clone());
            timestamps.insert(id.clone(), Utc::now());
        }

        // Set entry point
        if let Some(entry_id) = first_hnsw_id {
            hnsw_index.set_entry_point(entry_id);
        }

        // Add vectors to IVF
        let mut inverted_lists: HashMap<ClusterId, InvertedList> = HashMap::new();
        for i in 0..num_centroids {
            inverted_lists.insert(ClusterId(i), InvertedList::new());
        }

        for (idx, (id, vector)) in test_vectors
            .iter()
            .skip(hnsw_count)
            .take(ivf_count)
            .enumerate()
        {
            let cluster_id = ClusterId(idx % num_centroids);
            let list = inverted_lists.get_mut(&cluster_id).unwrap();
            list.insert(id.clone(), vector.clone())
                .expect("Failed to insert to IVF");
            vectors.push(vector.clone());
            timestamps.insert(id.clone(), Utc::now());
        }

        ivf_index.set_inverted_lists(inverted_lists);

        // Construct HybridIndex
        let index = HybridIndex::from_parts(
            config,
            hnsw_index,
            ivf_index,
            timestamps,
            hnsw_count,
            ivf_count,
        )
        .expect("Failed to create hybrid index");

        (index, vectors)
    })
}

/// Setup and save index to S5 storage
fn setup_and_save_index(
    rt: &Runtime,
    vector_count: usize,
) -> (MockS5Storage, HybridPersister<MockS5Storage>, Vec<Vec<f32>>) {
    rt.block_on(async {
        let (index, vectors) = setup_benchmark_index(rt, vector_count, DIMENSIONS);
        let storage = MockS5Storage::new();
        let persister = HybridPersister::new(storage.clone());

        // Save index
        persister
            .save_index_chunked(&index, "bench")
            .await
            .expect("Failed to save index");

        (storage, persister, vectors)
    })
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark: Cold cache search (first search after load)
fn bench_cold_cache_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, persister, vectors) = setup_and_save_index(&rt, BENCHMARK_VECTOR_COUNT);

    let query_vector = vectors[0].clone();

    c.bench_function("cold_cache_search", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Load fresh index (simulates cold cache)
                let loaded_index = persister
                    .load_index_chunked("bench")
                    .await
                    .expect("Failed to load index");

                // Perform first search
                let results = loaded_index
                    .search(black_box(&query_vector), 10)
                    .await
                    .expect("Search failed");

                black_box(results);
            });
        });
    });
}

/// Benchmark: Warm cache search (repeated searches)
fn bench_warm_cache_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (_storage, persister, vectors) = setup_and_save_index(&rt, BENCHMARK_VECTOR_COUNT);

    // Load index once
    let loaded_index = rt.block_on(async {
        persister
            .load_index_chunked("bench")
            .await
            .expect("Failed to load index")
    });

    let query_vector = vectors[0].clone();

    // Warm up cache with one search
    rt.block_on(async {
        let _ = loaded_index.search(&query_vector, 10).await;
    });

    c.bench_function("warm_cache_search", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = loaded_index
                    .search(black_box(&query_vector), 10)
                    .await
                    .expect("Search failed");

                black_box(results);
            });
        });
    });
}

/// Benchmark: Chunk loading overhead (cache miss penalty)
fn bench_chunk_loading_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (_storage, persister, vectors) = setup_and_save_index(&rt, BENCHMARK_VECTOR_COUNT);

    let loaded_index = rt.block_on(async {
        persister
            .load_index_chunked("bench")
            .await
            .expect("Failed to load index")
    });

    let mut group = c.benchmark_group("chunk_loading");
    group.measurement_time(Duration::from_secs(10));

    // Benchmark: First search (potential cache miss)
    let query_cold = vectors[0].clone();
    group.bench_function("first_search", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = loaded_index
                    .search(black_box(&query_cold), 10)
                    .await
                    .expect("Search failed");
                black_box(results);
            });
        });
    });

    // Benchmark: Repeated search (cache hit)
    let query_warm = vectors[0].clone();
    // Warm up
    rt.block_on(async {
        let _ = loaded_index.search(&query_warm, 10).await;
    });

    group.bench_function("cached_search", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = loaded_index
                    .search(black_box(&query_warm), 10)
                    .await
                    .expect("Search failed");
                black_box(results);
            });
        });
    });

    group.finish();
}

/// Benchmark: Load time comparison (chunked vs baseline)
fn bench_load_time_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("load_time");
    group.measurement_time(Duration::from_secs(15));

    // Test different dataset sizes
    for &size in &[1_000, 5_000, 10_000] {
        let (storage, persister, _vectors) = setup_and_save_index(&rt, size);

        group.bench_with_input(BenchmarkId::new("chunked_load", size), &size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let loaded_index = persister
                        .load_index_chunked("bench")
                        .await
                        .expect("Failed to load index");

                    black_box(loaded_index);
                });
            });
        });
    }

    group.finish();
}

/// Benchmark: Cache hit rate over 1000 searches
fn bench_cache_hit_rate(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (_storage, persister, vectors) = setup_and_save_index(&rt, BENCHMARK_VECTOR_COUNT);

    let loaded_index = rt.block_on(async {
        persister
            .load_index_chunked("bench")
            .await
            .expect("Failed to load index")
    });

    c.bench_function("cache_hit_rate_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Perform 1000 searches with varying query vectors
                for i in 0..1000 {
                    let query_idx = i % vectors.len();
                    let query = &vectors[query_idx];

                    let results = loaded_index
                        .search(black_box(query), 10)
                        .await
                        .expect("Search failed");

                    black_box(results);
                }
            });
        });
    });
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(3));
    targets =
        bench_cold_cache_search,
        bench_warm_cache_search,
        bench_chunk_loading_overhead,
        bench_load_time_comparison,
        bench_cache_hit_rate
);

criterion_main!(benches);
