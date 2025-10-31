// Standalone test to verify hybrid deletion implementation
// Run with: cargo run --example test_deletion

use chrono::Utc;
use vector_db::core::types::VectorId;
use vector_db::hybrid::core::{HybridConfig, HybridIndex};

#[tokio::main]
async fn main() {
    println!("🧪 Testing Hybrid Deletion Implementation\n");

    // Create hybrid index
    let config = HybridConfig {
        recent_threshold: std::time::Duration::from_secs(7 * 24 * 3600),
        hnsw_config: vector_db::hnsw::core::HNSWConfig::default(),
        ivf_config: vector_db::ivf::core::IVFConfig {
            n_clusters: 4,
            n_probe: 4,
            train_size: 100,
            max_iterations: 10,
            seed: Some(42),
        },
        migration_batch_size: 100,
        auto_migrate: false,
    };

    let mut index = HybridIndex::new(config);

    // Train index
    println!("📚 Training index...");
    let training_data: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..384).map(|j| ((i + j) as f32 * 0.01)).collect())
        .collect();
    index.initialize(training_data).await.unwrap();

    // Insert recent vectors (HNSW)
    println!("📝 Inserting 10 recent vectors (HNSW)...");
    let now = Utc::now();
    for i in 0..10 {
        let id = VectorId::from_string(&format!("recent_{}", i));
        let vector: Vec<f32> = (0..384).map(|j| ((i + j) as f32 * 0.01)).collect();
        let timestamp = now - chrono::Duration::hours(i as i64);
        index
            .insert_with_timestamp(id, vector, timestamp)
            .await
            .unwrap();
    }

    // Insert historical vectors (IVF)
    println!("📝 Inserting 10 historical vectors (IVF)...");
    for i in 0..10 {
        let id = VectorId::from_string(&format!("historical_{}", i));
        let vector: Vec<f32> = (0..384).map(|j| ((i + 100 + j) as f32 * 0.01)).collect();
        let timestamp = now - chrono::Duration::days(30 + i as i64);
        index
            .insert_with_timestamp(id, vector, timestamp)
            .await
            .unwrap();
    }

    // Test 1: active_count
    println!("\n✅ Test 1: active_count()");
    let count = index.active_count().await;
    assert_eq!(count, 20, "Should have 20 active vectors");
    println!("   Active count: {} ✓", count);

    // Test 2: Delete from recent index (HNSW)
    println!("\n✅ Test 2: delete() from recent index (HNSW)");
    let id = VectorId::from_string("recent_5");
    index.delete(id.clone()).await.unwrap();
    assert!(index.is_deleted(&id).await, "Vector should be deleted");
    println!("   Deleted 'recent_5' from HNSW ✓");

    // Test 3: Delete from historical index (IVF)
    println!("\n✅ Test 3: delete() from historical index (IVF)");
    let id = VectorId::from_string("historical_5");
    index.delete(id.clone()).await.unwrap();
    assert!(index.is_deleted(&id).await, "Vector should be deleted");
    println!("   Deleted 'historical_5' from IVF ✓");

    // Test 4: batch_delete
    println!("\n✅ Test 4: batch_delete()");
    let ids = vec![
        VectorId::from_string("recent_0"),
        VectorId::from_string("historical_0"),
        VectorId::from_string("nonexistent"),
    ];
    let stats = index.batch_delete(&ids).await.unwrap();
    assert_eq!(stats.successful, 2, "Should delete 2 vectors");
    assert_eq!(stats.failed, 1, "Should fail 1 deletion");
    println!(
        "   Batch deleted: {} successful, {} failed ✓",
        stats.successful, stats.failed
    );

    // Test 5: active_count after deletions
    println!("\n✅ Test 5: active_count() after deletions");
    let count = index.active_count().await;
    assert_eq!(count, 16, "Should have 16 active vectors (20 - 4)");
    println!("   Active count: {} ✓", count);

    // Test 6: Search excludes deleted vectors
    println!("\n✅ Test 6: Search excludes deleted vectors");
    let query: Vec<f32> = (0..384).map(|j| (j as f32 * 0.01)).collect();
    let results = index.search(&query, 10).await.unwrap();

    let deleted_ids = vec![
        VectorId::from_string("recent_5"),
        VectorId::from_string("historical_5"),
        VectorId::from_string("recent_0"),
        VectorId::from_string("historical_0"),
    ];

    for result in &results {
        assert!(
            !deleted_ids.contains(&result.vector_id),
            "Search should not return deleted vectors"
        );
    }
    println!("   Search returned {} results (no deleted vectors) ✓", results.len());

    // Test 7: vacuum
    println!("\n✅ Test 7: vacuum()");
    let vacuum_stats = index.vacuum().await.unwrap();
    println!(
        "   Vacuumed {} vectors ({} HNSW + {} IVF) ✓",
        vacuum_stats.total_removed,
        vacuum_stats.hnsw_removed,
        vacuum_stats.ivf_removed
    );
    assert_eq!(vacuum_stats.total_removed, 4, "Should remove 4 vectors");

    // Test 8: active_count after vacuum
    println!("\n✅ Test 8: active_count() after vacuum");
    let count = index.active_count().await;
    assert_eq!(count, 16, "Should still have 16 active vectors");
    println!("   Active count: {} ✓", count);

    // Test 9: Deleted vectors no longer exist after vacuum
    println!("\n✅ Test 9: is_deleted() returns false after vacuum");
    let id = VectorId::from_string("recent_5");
    assert!(
        !index.is_deleted(&id).await,
        "Vacuumed vectors should not be marked as deleted"
    );
    println!("   is_deleted('recent_5') = false ✓");

    println!("\n🎉 All tests passed!");
}
