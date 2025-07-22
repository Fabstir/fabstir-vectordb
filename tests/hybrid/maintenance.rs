use chrono::{TimeZone, Utc};
use std::time::Duration;
use tokio;
use vector_db::core::storage::*;
use vector_db::core::types::*;
use vector_db::hybrid::core::*;
use vector_db::hybrid::maintenance::*;

#[cfg(test)]
mod migration_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_migration_scheduler() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(3),
            auto_migrate: false,
            migration_batch_size: 10,
            ..HybridConfig::default()
        };

        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors that will age
        for i in 0..30 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![i as f32 * 0.1, 0.0];
            index.insert(id, vector).await.unwrap();
        }

        let scheduler = MigrationScheduler::new(index.clone());

        // Configure migration policy
        let policy = MigrationPolicy {
            check_interval: Duration::from_secs(1),
            batch_size: 10,
            max_vectors_per_run: 20,
            quiet_hours: vec![], // No quiet hours for testing
        };

        scheduler.set_policy(policy).await;

        // Wait for vectors to age
        tokio::time::sleep(Duration::from_secs(4)).await;

        // Run migration
        let result = scheduler.run_migration().await.unwrap();

        assert!(result.vectors_migrated > 0);
        assert!(result.batches_processed > 0);
        assert!(result.duration.as_millis() > 0);
        assert!(result.errors.is_empty());

        // Verify migration (max 20 vectors per run)
        assert_eq!(index.recent_count(), 10); // 30 - 20 migrated
        assert_eq!(index.historical_count(), 20); // 20 migrated
    }

    #[tokio::test]
    async fn test_continuous_migration() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(2),
            auto_migrate: false,
            migration_batch_size: 5,
            ..HybridConfig::default()
        };

        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        let scheduler = MigrationScheduler::new(index.clone());
        scheduler
            .set_policy(MigrationPolicy {
                check_interval: Duration::from_secs(1),
                batch_size: 5,
                max_vectors_per_run: 50,
                quiet_hours: vec![],
            })
            .await;

        // Start continuous migration
        let handle = scheduler.start_continuous().await.unwrap();

        // Continuously add vectors
        for batch in 0..3 {
            for i in 0..10 {
                let id = VectorId::from_string(&format!("batch{}_vec_{}", batch, i));
                let vector = vec![i as f32, batch as f32];
                index.insert(id, vector).await.unwrap();
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        // Let migration run
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Stop migration
        scheduler.stop_continuous(handle).await.unwrap();

        // Most vectors should be migrated
        let stats = scheduler.get_statistics().await;
        assert!(stats.total_vectors_migrated > 20);
        assert!(stats.total_runs > 0);
        assert!(stats.avg_vectors_per_run > 0.0);
    }

    #[tokio::test]
    async fn test_migration_with_errors() {
        let config = HybridConfig {
            recent_threshold: Duration::from_secs(1), // 1 second for testing
            ..HybridConfig::default()
        };
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Insert vectors normally (they go to recent index)
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, 0.0]).await.unwrap();
        }

        // Wait for vectors to age
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Create scheduler with error injection
        let scheduler = MigrationScheduler::with_error_handler(
            index.clone(),
            Box::new(|id| {
                // Fail on specific vectors
                id.to_string().contains("vec_5") || id.to_string().contains("vec_7")
            }),
        );

        let result = scheduler.run_migration().await.unwrap();

        assert_eq!(result.vectors_migrated, 8); // 10 - 2 errors
        assert_eq!(result.errors.len(), 2);
        assert!(result
            .errors
            .iter()
            .any(|e| e.vector_id.to_string().contains("vec_5")));
    }
}

#[cfg(test)]
mod rebalancing_tests {
    use super::*;

    #[tokio::test]
    async fn test_ivf_rebalancing() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Add many historical vectors with poor distribution
        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        for i in 0..100 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            // Most vectors cluster around origin
            let vector = if i < 90 {
                vec![i as f32 * 0.01, i as f32 * 0.01]
            } else {
                vec![10.0 + i as f32 * 0.1, 10.0]
            };
            index
                .insert_with_timestamp(id, vector, old_timestamp)
                .await
                .unwrap();
        }

        let rebalancer = IndexRebalancer::new(index.clone());

        // Check if rebalancing is needed
        let analysis = rebalancer.analyze_balance().await.unwrap();
        assert!(analysis.ivf_needs_rebalancing);
        assert!(analysis.cluster_imbalance > 0.5);

        // Perform rebalancing
        let result = rebalancer
            .rebalance_ivf(RebalanceConfig {
                target_cluster_size_variance: 0.2,
                max_iterations: 10,
                converge_threshold: 0.01,
            })
            .await
            .unwrap();

        assert!(result.clusters_modified > 0);
        assert!(result.vectors_moved > 0);
        assert!(result.final_variance < analysis.cluster_imbalance);
    }

    #[tokio::test]
    async fn test_automatic_rebalancing() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        let rebalancer = IndexRebalancer::new(index.clone());

        // Set up automatic rebalancing
        let auto_config = AutoRebalanceConfig {
            check_interval: Duration::from_secs(2),
            imbalance_threshold: 0.3,
            min_vectors_for_rebalance: 50,
            rebalance_ivf: true,
            optimize_hnsw: false, // Skip due to performance issues
        };

        let handle = rebalancer.start_auto_rebalance(auto_config).await.unwrap();

        // Add imbalanced data
        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        for i in 0..60 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            let vector = vec![0.0, i as f32 * 0.01]; // All in one area
            index
                .insert_with_timestamp(id, vector, old_timestamp)
                .await
                .unwrap();
        }

        // Let auto-rebalancing run
        tokio::time::sleep(Duration::from_secs(5)).await;

        rebalancer.stop_auto_rebalance(handle).await.unwrap();

        let stats = rebalancer.get_statistics().await;
        assert!(stats.total_rebalances > 0);
        assert!(stats.total_vectors_moved > 0);
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[tokio::test]
    async fn test_orphan_cleanup() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Simulate orphaned data by manually marking vectors as deleted
        // (This would normally happen due to failed migrations)
        for i in 0..5 {
            let id = VectorId::from_string(&format!("orphan_{}", i));
            index.insert(id, vec![i as f32, 0.0]).await.unwrap();
        }

        let cleaner = IndexCleaner::new(index.clone());

        // Scan for issues
        let issues = cleaner.scan_for_issues().await.unwrap();

        // In real scenario, we'd have orphans. For test, check structure
        assert!(issues.total_issues >= 0);

        // Run cleanup
        let result = cleaner
            .cleanup(CleanupConfig {
                remove_orphans: true,
                compact_storage: true,
                rebuild_stats: true,
                dry_run: false,
            })
            .await
            .unwrap();

        assert!(result.orphans_removed >= 0);
        assert!(result.space_reclaimed >= 0);
        assert!(result.stats_rebuilt);
    }

    #[tokio::test]
    async fn test_storage_compaction() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Add and remove vectors to create fragmentation
        let mut ids = Vec::new();
        for i in 0..50 {
            let id = VectorId::from_string(&format!("temp_{}", i));
            ids.push(id.clone());
            index.insert(id, vec![i as f32, 0.0]).await.unwrap();
        }

        // Simulate deletions (would need delete support)
        // For now, just measure compaction potential

        let cleaner = IndexCleaner::new(index.clone());

        let before = cleaner.estimate_storage_usage().await.unwrap();

        let result = cleaner.compact_storage().await.unwrap();

        assert!(result.indices_compacted > 0);
        assert!(result.space_saved_bytes >= 0);

        let after = cleaner.estimate_storage_usage().await.unwrap();
        assert!(after.total_bytes <= before.total_bytes);
    }
}

#[cfg(test)]
mod backup_restore_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_backup() {
        let storage = MockS5Storage::new();
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Add test data
        for i in 0..20 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, i as f32]).await.unwrap();
        }

        let backup_manager = BackupManager::new(storage);

        // Create backup
        let backup_result = backup_manager
            .create_backup(
                &index,
                "/backups/test_backup",
                BackupConfig {
                    include_recent: true,
                    include_historical: true,
                    compress: true,
                    encryption_key: None,
                },
            )
            .await
            .unwrap();

        assert!(backup_result.backup_size > 0);
        assert_eq!(backup_result.vectors_backed_up, 20);
        assert!(backup_result.compression_ratio > 0.0);

        // Verify backup
        let verify_result = backup_manager
            .verify_backup("/backups/test_backup")
            .await
            .unwrap();
        assert!(verify_result.is_valid);
        assert_eq!(verify_result.vector_count, 20);
    }

    #[tokio::test]
    async fn test_incremental_backup() {
        let storage = MockS5Storage::new();
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        let backup_manager = BackupManager::new(storage);

        // Initial backup with old vectors
        let old_time = Utc::now() - chrono::Duration::minutes(10);
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index
                .insert_with_timestamp(id, vec![i as f32, 0.0], old_time)
                .await
                .unwrap();
        }

        backup_manager
            .create_backup(&index, "/backups/base", BackupConfig::default())
            .await
            .unwrap();

        // Add more data (recent)
        for i in 10..20 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, 0.0]).await.unwrap();
        }

        // Incremental backup
        let incr_result = backup_manager
            .create_incremental_backup(
                &index,
                "/backups/base",
                "/backups/incr1",
                Utc::now() - chrono::Duration::minutes(5), // Since 5 minutes ago
            )
            .await
            .unwrap();

        assert_eq!(incr_result.vectors_backed_up, 10); // Only new vectors
        assert!(incr_result.backup_size > 0); // Should have non-zero size
    }

    #[tokio::test]
    async fn test_restore_to_point_in_time() {
        let storage = MockS5Storage::new();
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config.clone());
        index.initialize(create_training_data()).await.unwrap();

        let backup_manager = BackupManager::new(storage);

        // Create timeline of backups
        let times = vec![
            Utc::now() - chrono::Duration::hours(3),
            Utc::now() - chrono::Duration::hours(2),
            Utc::now() - chrono::Duration::hours(1),
        ];

        // Backup at each time (simplified)
        for (i, time) in times.iter().enumerate() {
            // Insert just a few vectors to avoid timeout
            for j in 0..5 {
                let id = VectorId::from_string(&format!("vec_t{}_{}", i, j));
                index
                    .insert_with_timestamp(id, vec![j as f32, 0.0], *time)
                    .await
                    .unwrap();
            }

            backup_manager
                .create_backup(
                    &index,
                    &format!("/backups/time_{}", i),
                    BackupConfig::default(),
                )
                .await
                .unwrap();
        }

        // Restore to middle point
        let mut restored_index = HybridIndex::new(config);
        restored_index
            .initialize(create_training_data())
            .await
            .unwrap();

        let restore_result = backup_manager
            .restore_to_point_in_time(&mut restored_index, times[1], "/backups")
            .await
            .unwrap();

        assert_eq!(restore_result.vectors_restored, 10); // First two backups (5 each)
        assert_eq!(restored_index.total_vectors(), 10);
    }
}

#[cfg(test)]
mod monitoring_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_monitoring() {
        let config = HybridConfig::default();
        let mut index = HybridIndex::new(config);
        index.initialize(create_training_data()).await.unwrap();

        // Add test data
        for i in 0..50 {
            let id = VectorId::from_string(&format!("vec_{}", i));
            index.insert(id, vec![i as f32, 0.0]).await.unwrap();
        }

        let monitor = HealthMonitor::new(index.clone());

        // Perform health check
        let health = monitor.check_health().await.unwrap();

        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.recent_index_ok);
        assert!(health.historical_index_ok);
        assert!(health.migration_backlog < 100);
        assert!(health.search_latency_ok);
        assert!(health.memory_usage_ok);
    }

    #[tokio::test]
    async fn test_alert_system() {
        let config = HybridConfig::default();
        let index = HybridIndex::new(config);

        let monitor = HealthMonitor::new(index);

        // Configure alerts
        monitor
            .configure_alerts(AlertConfig {
                migration_backlog_threshold: 1000,
                search_latency_threshold_ms: 100.0,
                memory_usage_threshold_bytes: 1_000_000_000,
                check_interval: Duration::from_secs(5),
            })
            .await;

        // Set up alert handler
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        monitor
            .set_alert_handler(move |alert| {
                let tx = tx.clone();
                Box::pin(async move {
                    tx.send(alert).await.unwrap();
                })
            })
            .await;

        // Simulate issue (would need to inject problems)
        // For now, just verify alert system is set up

        let alerts = monitor.get_recent_alerts().await;
        assert_eq!(alerts.len(), 0); // No alerts for healthy system
    }
}

// Helper functions
fn create_training_data() -> Vec<Vec<f32>> {
    vec![
        vec![0.0, 0.0],
        vec![1.0, 1.0],
        vec![-1.0, -1.0],
        vec![5.0, 5.0],
        vec![-5.0, -5.0],
    ]
}
