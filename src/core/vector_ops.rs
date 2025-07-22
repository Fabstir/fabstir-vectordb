use crate::core::types::{Embedding, SearchResult};

pub fn batch_cosine_similarity(query: &Embedding, vectors: &[Embedding]) -> Vec<f32> {
    vectors.iter()
        .map(|v| query.cosine_similarity(v))
        .collect()
}

pub fn top_k_indices(scores: &[f32], k: usize) -> Vec<usize> {
    let mut indexed_scores: Vec<(usize, f32)> = scores.iter()
        .enumerate()
        .map(|(i, &score)| (i, score))
        .collect();
    
    indexed_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    
    indexed_scores.iter()
        .take(k)
        .map(|(i, _)| *i)
        .collect()
}

pub fn merge_search_results(mut results: Vec<Vec<SearchResult>>, k: usize) -> Vec<SearchResult> {
    let mut all_results = Vec::new();
    for mut result_set in results.drain(..) {
        all_results.append(&mut result_set);
    }
    
    let deduped = SearchResult::deduplicate(all_results);
    deduped.into_iter().take(k).collect()
}

#[cfg(feature = "simd")]
pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    dot_product_scalar(a, b)
}

pub fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}