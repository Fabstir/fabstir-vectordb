use chrono::Utc;
use std::time::{Duration, SystemTime};
use tokio;
use vector_db::core::storage::*;
use vector_db::core::types::*;
use vector_db::hnsw::core::{HNSWConfig, HNSWIndex};
use vector_db::hybrid::core::*;
use vector_db::ivf::core::{IVFConfig, IVFIndex};

#[cfg(test)]
mod hybrid_structure_tests {
    use super::*;

    #[test]
    fn test_hybrid_config_creation() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(7 * 24 * 3600), // 7 days
            hnsw_config: HNSWConfig {
                max_connections: 16,
                max_connections_layer_0: 32,
                ef_construction: 200,
                seed: Some(42),
            },
            ivf_config: IVFConfig {
                n_clusters: 100,
                n_probe: 10,
                train_size: 1000,
                max_iterations: 25,
                seed: Some(42),
            },
            migration_batch_size: 100,
            auto_migrate: true,
        };

        assert_eq!(config.recent_threshold, Duration::from_secs(7 * 24 * 3600));
        assert!(config.auto_migrate);
        assert!(config.is_valid());
    }

    #[test]
    fn test_hybrid_index_initialization() {
        let config = HybridConfig::default();
        let index = HybridIndex::new(config.clone());

        assert_eq!(index.config(), &config);
        assert_eq!(index.recent_count(), 0);
        assert_eq!(index.historical_count(), 0);
        assert_eq!(index.total_vectors(), 0);
    }

    #[test]
    fn test_timestamped_vector_creation() {
        let id = VectorId::from_string("video_123");
        let vector = vec![1.0, 2.0, 3.0];
        let timestamp = Utc::now();

        let tv = TimestampedVector::new(id.clone(), vector.clone(), timestamp);

        assert_eq!(tv.id(), &id);
        assert_eq!(tv.vector(), &vector);
        // Compare timestamps by converting both to SystemTime
        let tv_time: SystemTime = tv.timestamp();
        let expected_time: SystemTime = timestamp.into();
        assert_eq!(tv_time, expected_time);
        assert!(tv.is_recent(Duration::from_secs(60))); // Recent within 1 minute
    }
}

#[cfg(test)]
mod hybrid_insertion_tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_recent_vector() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        // Initialize with training data
        index.initialize(create_training_data()).await.unwrap();

        let id = VectorId::from_string("recent_video");
        let vector = vec![1.0, 2.0];

        index
            .insert_with_timestamp(id.clone(), vector, Utc::now())
            .await
            .unwrap();

        assert_eq!(index.recent_count(), 1);
        assert_eq!(index.historical_count(), 0);
        assert_eq!(index.total_vectors(), 1);

        // Should be in HNSW index
        assert!(index.is_in_recent(&id));
    }

    #[tokio::test]
    async fn test_insert_historical_vector() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        let id = VectorId::from_string("old_video");
        let vector = vec![5.0, 5.0];
        let old_timestamp = Utc::now() - chrono::Duration::days(30);

        index
            .insert_with_timestamp(id.clone(), vector, old_timestamp)
            .await
            .unwrap();

        assert_eq!(index.recent_count(), 0);
        assert_eq!(index.historical_count(), 1);
        assert_eq!(index.total_vectors(), 1);

        // Should be in IVF index
        assert!(index.is_in_historical(&id));
    }

    #[tokio::test]
    async fn test_insert_without_timestamp() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        let id = VectorId::from_string("auto_timestamp");
        let vector = vec![1.0, 1.0];

        // Insert without timestamp (should use current time)
        index.insert(id.clone(), vector).await.unwrap();

        assert_eq!(index.recent_count(), 1);
        assert!(index.is_in_recent(&id));
    }

    #[tokio::test]
    async fn test_insert_duplicate() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        let id = VectorId::from_string("dup");
        let vector = vec![1.0, 1.0];

        index.insert(id.clone(), vector.clone()).await.unwrap();

        // Try to insert duplicate
        let result = index.insert(id, vector).await;
        assert!(result.is_err());
        match result {
            Err(HybridError::DuplicateVector(_)) => {}
            _ => panic!("Expected DuplicateVector error"),
        }
    }
}

#[cfg(test)]
mod hybrid_search_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_empty_index() {
        let config = HybridConfig::default();
        let index = HybridIndex::new(config);

        let results = index.search(&vec![1.0, 2.0], 5).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_search_recent_only() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert only recent vectors
        for i in 0..5 {
            let id = VectorId::from_string(&format!("recent_{}", i));
            let vector = vec![i as f32, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        let results = index.search(&vec![2.5, 0.0], 3).await.unwrap();

        assert_eq!(results.len(), 3);
        // Should find vectors near 2.5
        println!(
            "Results: {:?}",
            results
                .iter()
                .map(|r| (r.vector_id.clone(), r.distance))
                .collect::<Vec<_>>()
        );
        assert!(results[0].distance <= results[1].distance);
        assert!(results[1].distance <= results[2].distance);
    }

    #[tokio::test]
    async fn test_search_historical_only() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert only historical vectors
        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        let mut historical_ids = Vec::new();
        for i in 0..5 {
            let id = VectorId::from_string(&format!("historical_{}", i));
            historical_ids.push(id.clone());
            let vector = vec![0.0, i as f32];
            index
                .insert_with_timestamp(id, vector, old_timestamp)
                .await
                .unwrap();
        }

        // Debug counts
        println!(
            "Recent count: {}, Historical count: {}",
            index.recent_count(),
            index.historical_count()
        );

        let results = index.search(&vec![0.0, 2.5], 3).await.unwrap();

        assert_eq!(results.len(), 3);
        // Check that all results are from our historical vectors
        assert!(results
            .iter()
            .all(|r| historical_ids.contains(&r.vector_id)));
    }

    #[tokio::test]
    async fn test_search_mixed() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert recent vectors
        let mut recent_ids = Vec::new();
        for i in 0..3 {
            let id = VectorId::from_string(&format!("recent_{}", i));
            recent_ids.push(id.clone());
            let vector = vec![i as f32, i as f32];
            index.insert(id, vector).await.unwrap();
        }

        // Insert historical vectors
        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        let mut historical_ids = Vec::new();
        for i in 3..6 {
            let id = VectorId::from_string(&format!("historical_{}", i));
            historical_ids.push(id.clone());
            let vector = vec![i as f32, i as f32];
            index
                .insert_with_timestamp(id, vector, old_timestamp)
                .await
                .unwrap();
        }

        // Search should find from both indices
        let results = index.search(&vec![2.5, 2.5], 6).await.unwrap();

        assert_eq!(results.len(), 6);

        // Check we have results from both indices
        let recent_results = results
            .iter()
            .filter(|r| recent_ids.contains(&r.vector_id))
            .count();
        let historical_results = results
            .iter()
            .filter(|r| historical_ids.contains(&r.vector_id))
            .count();

        assert!(recent_results > 0);
        assert!(historical_results > 0);
        assert_eq!(recent_results + historical_results, 6);
    }

    #[tokio::test]
    async fn test_search_with_config() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32 * 0.1, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        let search_config = HybridSearchConfig {
            k: 5,
            hnsw_ef: 100,
            ivf_n_probe: 5,
            search_recent: true,
            search_historical: true,
            ..HybridSearchConfig::default()
        };

        let results = index
            .search_with_config(&vec![0.5, 0.0], search_config)
            .await
            .unwrap();

        assert!(results.len() <= 5);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[tokio::test]
    async fn test_manual_migration() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(3), // 3 seconds for testing
            auto_migrate: false,
            ..HybridConfig::default()
        };

        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors
        for i in 0..5 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        assert_eq!(index.recent_count(), 5);
        assert_eq!(index.historical_count(), 0);

        // Wait for vectors to become old
        tokio::time::sleep(Duration::from_secs(4)).await;

        // Manually trigger migration
        let result = index.migrate_old_vectors().await.unwrap();

        assert_eq!(result.vectors_migrated, 5);
        assert_eq!(index.recent_count(), 0);
        assert_eq!(index.historical_count(), 5);
    }

    #[tokio::test]
    async fn test_auto_migration() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(2), // 2 seconds for testing
            auto_migrate: true,
            ..HybridConfig::default()
        };

        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors
        for i in 0..3 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        // Immediately update timestamps to make them old
        {
            let mut timestamps = index.timestamps.write().await;
            let old_timestamp = Utc::now() - chrono::Duration::seconds(3);
            for (_, timestamp) in timestamps.iter_mut() {
                *timestamp = old_timestamp;
            }
        }

        assert_eq!(index.recent_count(), 3);

        // Start auto-migration
        index.start_auto_migration().await.unwrap();

        // Wait for migration to happen
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Vectors should have been migrated
        assert_eq!(index.recent_count(), 0);
        assert_eq!(index.historical_count(), 3);

        // Stop auto-migration
        index.stop_auto_migration().await.unwrap();
    }

    #[tokio::test]
    async fn test_migration_during_search() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(2),
            auto_migrate: false,
            ..HybridConfig::default()
        };

        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors that will become old
        for i in 0..5 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        // Wait for them to become old
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Search should still find them (triggers migration if needed)
        let results = index.search(&vec![2.5, 0.0], 5).await.unwrap();
        assert_eq!(results.len(), 5);
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;

    #[tokio::test]
    async fn test_index_statistics() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert mixed vectors
        for i in 0..10 {
            let id = VectorId::from_string(&format!("recent_{}", i));
            let vector = vec![i as f32, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        for i in 0..15 {
            let id = VectorId::from_string(&format!("historical_{}", i));
            let vector = vec![0.0, i as f32];
            index
                .insert_with_timestamp(id, vector, old_timestamp)
                .await
                .unwrap();
        }

        let stats = index.get_statistics().await;

        assert_eq!(stats.total_vectors, 25);
        assert_eq!(stats.recent_vectors, 10);
        assert_eq!(stats.historical_vectors, 15);
        assert!(stats.recent_index_memory > 0);
        assert!(stats.historical_index_memory > 0);
        assert!(stats.avg_query_time_ms >= 0.0);
    }

    #[tokio::test]
    async fn test_age_distribution() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);

        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors with different ages
        let now = Utc::now();
        let timestamps = vec![
            now,
            now - chrono::Duration::hours(1),
            now - chrono::Duration::days(1),
            now - chrono::Duration::days(7),
            now - chrono::Duration::days(30),
        ];

        for (i, timestamp) in timestamps.iter().enumerate() {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32, 0.0];
            index
                .insert_with_timestamp(id, vector, *timestamp)
                .await
                .unwrap();
        }

        let distribution = index.get_age_distribution().await.unwrap();

        assert_eq!(distribution.total_vectors, 5);
        assert!(distribution.buckets.len() > 0);
        assert_eq!(distribution.newest_timestamp, now);
        assert_eq!(
            distribution.oldest_timestamp,
            now - chrono::Duration::days(30)
        );
    }
}

// Helper functions
fn create_training_data() -> Vec<Vec<f32>> {
    vec![
        vec![0.0, 0.0],
        vec![0.1, 0.1],
        vec![0.2, -0.1],
        vec![5.0, 5.0],
        vec![5.1, 4.9],
        vec![4.9, 5.1],
        vec![-5.0, -5.0],
        vec![-4.9, -5.1],
        vec![-5.1, -4.9],
    ]
}
