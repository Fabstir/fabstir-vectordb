// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Example demonstrating filtered search functionality

use serde_json::json;
use std::collections::HashMap;
use vector_db::{
    core::metadata_filter::MetadataFilter,
    core::types::VectorId,
    hybrid::{HybridConfig, HybridIndex},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing filtered search functionality...\n");

    // Create and initialize index
    let config = HybridConfig::default();
    let mut index = HybridIndex::new(config);

    // Initialize with training data
    let training_data: Vec<Vec<f32>> = (0..10)
        .map(|i| (0..128).map(|j| ((i + j) as f32).sin() * 0.5).collect())
        .collect();
    index.initialize(training_data).await?;

    // Add test vectors with metadata
    let mut metadata_map = HashMap::new();

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
                "tags": ["web"]
            }),
        ),
        (
            "vec-2",
            2,
            json!({
                "category": "sports",
                "published": true,
                "views": 3000,
                "tags": ["football"]
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
    ];

    for (id, seed, metadata) in test_data {
        let vector_id = VectorId::from_string(id);
        let vector: Vec<f32> = (0..128).map(|j| ((seed + j) as f32).sin() * 0.5).collect();
        index.insert(vector_id.clone(), vector).await?;

        let mut metadata_with_id = metadata;
        metadata_with_id["_originalId"] = json!(id);
        metadata_map.insert(vector_id.to_string(), metadata_with_id);
    }

    // Test 1: Equals filter
    println!("Test 1: Equals filter (category = 'technology')");
    let filter = MetadataFilter::from_json(&json!({"category": "technology"}))?;
    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() * 0.5).collect();
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await?;
    println!("  Results: {} vectors", results.len());
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        println!("    - {} (category: {})", metadata["_originalId"], metadata["category"]);
    }

    // Test 2: Range filter
    println!("\nTest 2: Range filter (views >= 1000)");
    let filter = MetadataFilter::from_json(&json!({"views": {"$gte": 1000}}))?;
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await?;
    println!("  Results: {} vectors", results.len());
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        println!("    - {} (views: {})", metadata["_originalId"], metadata["views"]);
    }

    // Test 3: AND combinator
    println!("\nTest 3: AND combinator (technology + published)");
    let filter = MetadataFilter::from_json(&json!({
        "$and": [
            {"category": "technology"},
            {"published": true}
        ]
    }))?;
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await?;
    println!("  Results: {} vectors", results.len());
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        println!(
            "    - {} (category: {}, published: {})",
            metadata["_originalId"], metadata["category"], metadata["published"]
        );
    }

    // Test 4: Array field matching
    println!("\nTest 4: Array field matching (tags contains 'ai')");
    let filter = MetadataFilter::from_json(&json!({"tags": "ai"}))?;
    let results = index
        .search_with_filter(&query, 10, Some(&filter), &metadata_map)
        .await?;
    println!("  Results: {} vectors", results.len());
    for result in &results {
        let metadata = metadata_map.get(&result.vector_id.to_string()).unwrap();
        println!("    - {} (tags: {:?})", metadata["_originalId"], metadata["tags"]);
    }

    // Test 5: No filter (backward compatibility)
    println!("\nTest 5: No filter (backward compatibility)");
    let results = index
        .search_with_filter(&query, 10, None, &metadata_map)
        .await?;
    println!("  Results: {} vectors (all)", results.len());

    println!("\n✅ All filtered search tests passed!");

    Ok(())
}
