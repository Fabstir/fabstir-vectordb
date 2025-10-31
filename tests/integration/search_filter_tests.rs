// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests for filtered search functionality
//!
//! Tests the integration of MetadataFilter with HybridIndex search operations.

use serde_json::json;
use std::collections::HashMap;
use vector_db::{
    core::{
        metadata_filter::MetadataFilter,
        storage::MockS5Storage,
        types::VectorId,
    },
    hybrid::{HybridConfig, HybridIndex},
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
) {
    let test_data = vec![
        (
            "vec-0",
            0,
            json!({
                "category": "technology",
                "published": true,
                "views": 1500,
                "tags": ["ai", "ml"]
            }),
        ),
        (
            "vec-1",
            1,
            json!({
                "category": "technology",
                "published": false,
                "views": 500,
                "tags": ["web", "frontend"]
            }),
        ),
        (
            "vec-2",
            2,
            json!({
                "category": "sports",
                "published": true,
                "views": 3000,
                "tags": ["football", "news"]
            }),
        ),
        (
            "vec-3",
            3,
            json!({
                "category": "technology",
                "published": true,
                "views": 5000,
                "tags": ["ai", "robotics"]
            }),
        ),
        (
            "vec-4",
            4,
            json!({
                "category": "sports",
                "published": true,
                "views": 800,
                "tags": ["basketball", "highlights"]
            }),
        ),
    ];

    for (id, seed, metadata) in test_data {
        let vector_id = VectorId::from_string(id);
        let vector: Vec<f32> = (0..128)
            .map(|j| ((seed + j) as f32).sin() * 0.5)
            .collect();

        index.insert(vector_id.clone(), vector).await.unwrap();

        let mut metadata_with_id = metadata;
        metadata_with_id["_originalId"] = json!(id);
        metadata_map.insert(vector_id.to_string(), metadata_with_id);
    }
}

#[tokio::test]
async fn test_search_with_equals_filter() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for technology category
    let filter = MetadataFilter::from_json(&json!({
        "category": "technology"
    }))
    .unwrap();

    // Search (query similar to vec-0)
    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should only return technology articles
    assert!(results.len() <= 3); // vec-0, vec-1, vec-3 are technology
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        assert_eq!(metadata["category"], "technology");
    }
}

#[tokio::test]
async fn test_search_with_in_filter() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for technology or sports
    let filter = MetadataFilter::from_json(&json!({
        "category": {
            "$in": ["technology", "sports"]
        }
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return all vectors (all are technology or sports)
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_search_with_range_filter() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for views >= 1000
    let filter = MetadataFilter::from_json(&json!({
        "views": {
            "$gte": 1000
        }
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return vec-0 (1500), vec-2 (3000), vec-3 (5000)
    assert!(results.len() <= 3);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        let views = metadata["views"].as_i64().unwrap();
        assert!(views >= 1000);
    }
}

#[tokio::test]
async fn test_search_with_and_combinator() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for published technology articles
    let filter = MetadataFilter::from_json(&json!({
        "$and": [
            {"category": "technology"},
            {"published": true}
        ]
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return vec-0 and vec-3 (technology + published)
    assert!(results.len() <= 2);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        assert_eq!(metadata["category"], "technology");
        assert_eq!(metadata["published"], true);
    }
}

#[tokio::test]
async fn test_search_with_or_combinator() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for high views OR published
    let filter = MetadataFilter::from_json(&json!({
        "$or": [
            {"views": {"$gte": 3000}},
            {"published": true}
        ]
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return vec-0, vec-2, vec-3, vec-4 (all published or high views)
    assert!(results.len() <= 4);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        let views = metadata["views"].as_i64().unwrap();
        let published = metadata["published"].as_bool().unwrap();
        assert!(views >= 3000 || published);
    }
}

#[tokio::test]
async fn test_search_with_no_matches() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for non-existent category
    let filter = MetadataFilter::from_json(&json!({
        "category": "finance"
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return empty results
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_search_with_k_oversample() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for technology (3 matches)
    let filter = MetadataFilter::from_json(&json!({
        "category": "technology"
    }))
    .unwrap();

    // Request only top 2, but oversample should get all 3 and filter down
    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 2, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return exactly 2 results (k=2)
    assert_eq!(results.len(), 2);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        assert_eq!(metadata["category"], "technology");
    }
}

#[tokio::test]
async fn test_search_no_filter_backward_compatibility() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // No filter - should return all results
    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 5, None, &metadata_map)
        .await
        .unwrap();

    // Should return all 5 vectors
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_filter_with_array_field() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for vectors with "ai" tag
    let filter = MetadataFilter::from_json(&json!({
        "tags": "ai"
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return vec-0 and vec-3 (both have "ai" tag)
    assert!(results.len() <= 2);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        let tags = metadata["tags"].as_array().unwrap();
        assert!(tags.contains(&json!("ai")));
    }
}

#[tokio::test]
async fn test_complex_filter_combination() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Complex filter: technology AND (published OR high views)
    let filter = MetadataFilter::from_json(&json!({
        "$and": [
            {"category": "technology"},
            {
                "$or": [
                    {"published": true},
                    {"views": {"$gte": 5000}}
                ]
            }
        ]
    }))
    .unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Should return vec-0 (tech+published) and vec-3 (tech+published+high views)
    assert!(results.len() <= 2);
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        assert_eq!(metadata["category"], "technology");

        let published = metadata["published"].as_bool().unwrap();
        let views = metadata["views"].as_i64().unwrap();
        assert!(published || views >= 5000);
    }
}

#[tokio::test]
async fn test_filter_preserves_ranking() {
    let mut index = create_test_index().await;
    let mut metadata_map = HashMap::new();
    add_test_vectors_with_metadata(&mut index, &mut metadata_map).await;

    // Filter for technology
    let filter = MetadataFilter::from_json(&json!({
        "category": "technology"
    }))
    .unwrap();

    // Query closest to vec-3
    let query: Vec<f32> = (0..128).map(|j| ((3 + j) as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await
        .unwrap();

    // Results should be ranked by similarity (vec-3 should be first among technology)
    assert!(results.len() > 0);

    // Verify distances are in ascending order
    for i in 1..results.len() {
        assert!(results[i - 1].distance <= results[i].distance);
    }
}
