// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use crate::core::types::{Embedding, SearchResult, VectorId};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub fn batch_cosine_similarity(query: &Embedding, vectors: &[Embedding]) -> Vec<f32> {
    vectors.iter().map(|v| query.cosine_similarity(v)).collect()
}

pub fn top_k_indices(scores: &[f32], k: usize) -> Vec<usize> {
    let mut indexed_scores: Vec<(usize, f32)> = scores
        .iter()
        .enumerate()
        .map(|(i, &score)| (i, score))
        .collect();

    indexed_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    indexed_scores.iter().take(k).map(|(i, _)| *i).collect()
}

pub fn merge_search_results(mut results: Vec<Vec<SearchResult>>, k: usize) -> Vec<SearchResult> {
    let mut all_results = Vec::new();
    for mut result_set in results.drain(..) {
        all_results.append(&mut result_set);
    }

    let deduped = SearchResult::deduplicate(all_results);
    deduped.into_iter().take(k).collect()
}

// Scalar implementations
pub fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn cosine_similarity_scalar(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product_scalar(a, b);
    let norm_a = dot_product_scalar(a, a).sqrt();
    let norm_b = dot_product_scalar(b, b).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

pub fn euclidean_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

// SIMD implementations
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    unsafe {
        let mut sum = _mm256_setzero_ps();
        let chunks = a.len() / 8;

        for i in 0..chunks {
            let a_vec = _mm256_loadu_ps(a.as_ptr().add(i * 8));
            let b_vec = _mm256_loadu_ps(b.as_ptr().add(i * 8));
            let prod = _mm256_mul_ps(a_vec, b_vec);
            sum = _mm256_add_ps(sum, prod);
        }

        // Sum the 8 floats in the AVX register
        let mut result = [0.0f32; 8];
        _mm256_storeu_ps(result.as_mut_ptr(), sum);
        let mut scalar_sum = result.iter().sum::<f32>();

        // Handle remaining elements
        for i in (chunks * 8)..a.len() {
            scalar_sum += a[i] * b[i];
        }

        scalar_sum
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    dot_product_scalar(a, b)
}

pub fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product_simd(a, b);
    let norm_a = dot_product_simd(a, a).sqrt();
    let norm_b = dot_product_simd(b, b).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

pub fn euclidean_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut sum = _mm256_setzero_ps();
        let chunks = a.len() / 8;

        for i in 0..chunks {
            let a_vec = _mm256_loadu_ps(a.as_ptr().add(i * 8));
            let b_vec = _mm256_loadu_ps(b.as_ptr().add(i * 8));
            let diff = _mm256_sub_ps(a_vec, b_vec);
            let squared = _mm256_mul_ps(diff, diff);
            sum = _mm256_add_ps(sum, squared);
        }

        let mut result = [0.0f32; 8];
        _mm256_storeu_ps(result.as_mut_ptr(), sum);
        let mut scalar_sum = result.iter().sum::<f32>();

        // Handle remaining elements
        for i in (chunks * 8)..a.len() {
            let diff = a[i] - b[i];
            scalar_sum += diff * diff;
        }

        scalar_sum.sqrt()
    }

    #[cfg(not(target_arch = "x86_64"))]
    euclidean_distance_scalar(a, b)
}

pub fn batch_normalize(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
    vectors
        .iter()
        .map(|v| {
            let norm = dot_product_scalar(v, v).sqrt();
            if norm == 0.0 {
                v.clone()
            } else {
                v.iter().map(|x| x / norm).collect()
            }
        })
        .collect()
}

// Heap-based top-k selection
#[derive(Debug, Clone)]
struct HeapItem {
    index: usize,
    score: f32,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for HeapItem {}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Min heap: smaller scores have higher priority
        other.score.partial_cmp(&self.score)
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

pub fn top_k_indices_heap(scores: &[f32], k: usize) -> Vec<usize> {
    if k == 0 {
        return vec![];
    }

    let mut heap = BinaryHeap::with_capacity(k);

    for (i, &score) in scores.iter().enumerate() {
        if heap.len() < k {
            heap.push(HeapItem { index: i, score });
        } else if let Some(min) = heap.peek() {
            if score > min.score {
                heap.pop();
                heap.push(HeapItem { index: i, score });
            }
        }
    }

    let mut results: Vec<_> = heap.into_iter().collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    results.into_iter().map(|item| item.index).collect()
}

// Streaming top-k
pub struct StreamingTopK {
    heap: BinaryHeap<(OrderedFloat, VectorId)>,
    k: usize,
}

#[derive(Debug, Clone, Copy)]
struct OrderedFloat(f32);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse order for min-heap behavior
        other.0.partial_cmp(&self.0)
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

impl StreamingTopK {
    pub fn new(k: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(k),
            k,
        }
    }

    pub fn add(&mut self, id: VectorId, score: f32) {
        if self.heap.len() < self.k {
            self.heap.push((OrderedFloat(score), id));
        } else if let Some((min_score, _)) = self.heap.peek() {
            // Because we're using a min-heap with reversed ordering,
            // peek() gives us the smallest score
            if score > min_score.0 {
                self.heap.pop();
                self.heap.push((OrderedFloat(score), id));
            }
        }
    }

    pub fn get_results(self) -> Vec<SearchResult> {
        let mut results: Vec<_> = self
            .heap
            .into_iter()
            .map(|(score, id)| SearchResult::new(id, score.0, None))
            .collect();
        results.sort_by(|a, b| b.distance.partial_cmp(&a.distance).unwrap());
        results
    }
}

// Parallel operations
use std::sync::Arc;
use tokio::task;

pub async fn compute_similarities_parallel(
    query: &Embedding,
    vectors: &[Embedding],
    num_threads: usize,
) -> Vec<f32> {
    let chunk_size = (vectors.len() + num_threads - 1) / num_threads;
    let query = Arc::new(query.clone());
    let vectors = Arc::new(vectors.to_vec());

    let mut tasks = vec![];

    for i in 0..num_threads {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(vectors.len());

        if start >= vectors.len() {
            break;
        }

        let query = Arc::clone(&query);
        let vectors = Arc::clone(&vectors);

        let task = task::spawn_blocking(move || {
            vectors[start..end]
                .iter()
                .map(|v| query.cosine_similarity(v))
                .collect::<Vec<f32>>()
        });

        tasks.push(task);
    }

    let mut results = vec![];
    for task in tasks {
        let mut chunk_results = task.await.unwrap();
        results.append(&mut chunk_results);
    }

    results
}

pub async fn batch_search_parallel(
    queries: &[Embedding],
    vectors: &[Embedding],
    k: usize,
) -> Vec<Vec<SearchResult>> {
    let mut tasks = vec![];

    for (_i, query) in queries.iter().enumerate() {
        let query = query.clone();
        let vectors = vectors.to_vec();

        let task = task::spawn_blocking(move || {
            let similarities = batch_cosine_similarity(&query, &vectors);
            let indices = top_k_indices_heap(&similarities, k);

            indices
                .into_iter()
                .map(|idx| {
                    SearchResult::new(
                        VectorId::from_string(&format!("vec_{}", idx)),
                        similarities[idx],
                        None,
                    )
                })
                .collect::<Vec<_>>()
        });

        tasks.push(task);
    }

    let mut results = vec![];
    for task in tasks {
        results.push(task.await.unwrap());
    }

    results
}

// Quantization
#[derive(Debug, Clone)]
pub struct ScalarQuantizedVector {
    pub data: Vec<u8>,
    pub min: f32,
    pub max: f32,
}

impl ScalarQuantizedVector {
    pub fn dequantize(&self) -> Vec<f32> {
        let range = self.max - self.min;
        self.data
            .iter()
            .map(|&byte| {
                let normalized = byte as f32 / 255.0;
                self.min + normalized * range
            })
            .collect()
    }
}

pub fn scalar_quantize_u8(vector: &[f32]) -> ScalarQuantizedVector {
    let min = vector.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = vector.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;

    let data = if range == 0.0 {
        vec![0; vector.len()]
    } else {
        vector
            .iter()
            .map(|&v| {
                let normalized = (v - min) / range;
                (normalized * 255.0).round() as u8
            })
            .collect()
    };

    ScalarQuantizedVector { data, min, max }
}

// Product Quantization
pub struct ProductQuantizer {
    subspace_dim: usize,
    num_centroids: usize,
    centroids: Vec<Vec<Vec<f32>>>, // [subspace][centroid][dim]
}

impl ProductQuantizer {
    pub fn new(num_subspaces: usize, num_centroids: usize) -> Self {
        Self {
            subspace_dim: num_subspaces,
            num_centroids,
            centroids: vec![],
        }
    }

    pub fn train(&mut self, vectors: &[Vec<f32>], iterations: usize) {
        if vectors.is_empty() {
            return;
        }

        let vector_dim = vectors[0].len();
        let subvector_dim = vector_dim / self.subspace_dim;

        self.centroids = vec![];

        for subspace in 0..self.subspace_dim {
            let start = subspace * subvector_dim;
            let end = if subspace == self.subspace_dim - 1 {
                vector_dim
            } else {
                (subspace + 1) * subvector_dim
            };

            // Extract subvectors
            let subvectors: Vec<Vec<f32>> =
                vectors.iter().map(|v| v[start..end].to_vec()).collect();

            // Simple k-means clustering
            // Adjust number of centroids based on available data
            let actual_num_centroids = self.num_centroids.min(subvectors.len());
            let mut centroids =
                self.initialize_centroids_with_size(&subvectors, actual_num_centroids);

            for _ in 0..iterations {
                let assignments = self.assign_to_centroids(&subvectors, &centroids);
                centroids = self.update_centroids_with_size(
                    &subvectors,
                    &assignments,
                    actual_num_centroids,
                );
            }

            self.centroids.push(centroids);
        }
    }

    fn initialize_centroids_with_size(
        &self,
        vectors: &[Vec<f32>],
        num_centroids: usize,
    ) -> Vec<Vec<f32>> {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();

        // Ensure we don't try to take more centroids than we have vectors
        let num_centroids = num_centroids.min(vectors.len());

        let mut indices: Vec<usize> = (0..vectors.len()).collect();
        indices.shuffle(&mut rng);

        indices
            .into_iter()
            .take(num_centroids)
            .map(|i| vectors[i].clone())
            .collect()
    }

    fn assign_to_centroids(&self, vectors: &[Vec<f32>], centroids: &[Vec<f32>]) -> Vec<usize> {
        vectors
            .iter()
            .map(|v| {
                centroids
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, euclidean_distance_scalar(v, c)))
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .unwrap()
                    .0
            })
            .collect()
    }

    fn update_centroids_with_size(
        &self,
        vectors: &[Vec<f32>],
        assignments: &[usize],
        num_centroids: usize,
    ) -> Vec<Vec<f32>> {
        if vectors.is_empty() {
            return vec![];
        }

        let dim = vectors[0].len();
        let mut new_centroids = vec![vec![0.0; dim]; num_centroids];
        let mut counts = vec![0; num_centroids];

        for (v, &assignment) in vectors.iter().zip(assignments) {
            if assignment < num_centroids {
                for (i, &val) in v.iter().enumerate() {
                    new_centroids[assignment][i] += val;
                }
                counts[assignment] += 1;
            }
        }

        // Handle empty clusters by reinitializing with random vectors
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();

        for (_i, (centroid, count)) in new_centroids.iter_mut().zip(counts).enumerate() {
            if count > 0 {
                for val in centroid.iter_mut() {
                    *val /= count as f32;
                }
            } else if !vectors.is_empty() {
                // Reinitialize empty cluster with a random vector
                if let Some(v) = vectors.choose(&mut rng) {
                    *centroid = v.clone();
                }
            }
        }

        new_centroids
    }

    pub fn encode(&self, vector: &[f32]) -> Vec<u8> {
        if self.centroids.is_empty() {
            return vec![];
        }

        let vector_dim = vector.len();
        let subvector_dim = vector_dim / self.subspace_dim;
        let mut codes = vec![];

        for (subspace, centroids) in self.centroids.iter().enumerate() {
            if centroids.is_empty() {
                codes.push(0);
                continue;
            }

            let start = subspace * subvector_dim;
            let end = if subspace == self.subspace_dim - 1 {
                vector_dim
            } else {
                (subspace + 1) * subvector_dim
            };

            let subvector = &vector[start..end];

            let closest = centroids
                .iter()
                .enumerate()
                .map(|(i, c)| (i, euclidean_distance_scalar(subvector, c)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap_or((0, 0.0))
                .0;

            codes.push(closest as u8);
        }

        codes
    }

    pub fn decode(&self, codes: &[u8]) -> Vec<f32> {
        let mut reconstructed = vec![];

        for (subspace, &code) in codes.iter().enumerate() {
            if subspace < self.centroids.len() {
                let centroids = &self.centroids[subspace];
                if !centroids.is_empty() && (code as usize) < centroids.len() {
                    let centroid = &centroids[code as usize];
                    reconstructed.extend_from_slice(centroid);
                }
            }
        }

        reconstructed
    }
}

// Distance corrections
pub fn inner_product_to_cosine(inner_product: f32, a: &[f32], b: &[f32]) -> f32 {
    let norm_a = dot_product_scalar(a, a).sqrt();
    let norm_b = dot_product_scalar(b, b).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        inner_product / (norm_a * norm_b)
    }
}

pub fn angular_distance(a: &[f32], b: &[f32]) -> f32 {
    let cosine = cosine_similarity_scalar(a, b);
    let clamped = cosine.max(-1.0).min(1.0);
    clamped.acos()
}
