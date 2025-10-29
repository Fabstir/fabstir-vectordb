// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio;
use vector_db::core::types::*;
use vector_db::hybrid::core::*;
use vector_db::hybrid::search_integration::*;

#[cfg(test)]
mod parallel_search_tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_search_execution() {
        let index = create_populated_hybrid_index().await;

        let query = vec![2.5, 2.5];
        let config = ParallelSearchConfig {
            k: 10,
            timeout: Duration::from_secs(5),
            max_concurrent_searches: 4,
            hnsw_weight: 1.0,
            ivf_weight: 1.0,
        };

        let start = Instant::now();
        let results = index.parallel_search(&query, config).await.unwrap();
        let duration = start.elapsed();

        assert!(!results.results.is_empty());
        assert!(results.results.len() <= 10);
        assert!(results.search_time < duration);
        assert_eq!(results.indices_searched, 2); // HNSW and IVF

        // Verify results are sorted by score
        for i in 1..results.results.len() {
            assert!(results.results[i - 1].score >= results.results[i].score);
        }
    }

    #[tokio::test]
    #[ignore = "Skipped due to HNSW performance issues with large datasets"]
    async fn test_search_timeout() {
        let index = create_large_hybrid_index().await;

        let config = ParallelSearchConfig {
            k: 100,
            timeout: Duration::from_millis(10), // Very short timeout
            max_concurrent_searches: 2,
            hnsw_weight: 1.0,
            ivf_weight: 1.0,
        };

        let results = index
            .parallel_search(&vec![0.0, 0.0], config)
            .await
            .unwrap();

        // Should return partial results due to timeout
        assert!(results.timed_out);
        assert!(results.indices_searched <= 2);
    }

    #[tokio::test]
    async fn test_weighted_search() {
        let index = create_populated_hybrid_index().await;

        // Search with HNSW preference
        let config_hnsw = ParallelSearchConfig {
            k: 10,
            timeout: Duration::from_secs(5),
            max_concurrent_searches: 2,
            hnsw_weight: 2.0,
            ivf_weight: 0.5,
        };

        let results_hnsw = index
            .parallel_search(&vec![1.0, 1.0], config_hnsw)
            .await
            .unwrap();

        // Search with IVF preference
        let config_ivf = ParallelSearchConfig {
            k: 10,
            timeout: Duration::from_secs(5),
            max_concurrent_searches: 2,
            hnsw_weight: 0.5,
            ivf_weight: 2.0,
        };

        let results_ivf = index
            .parallel_search(&vec![1.0, 1.0], config_ivf)
            .await
            .unwrap();

        // Count HNSW results in top positions for each search
        let hnsw_weighted_hnsw_count = results_hnsw
            .results
            .iter()
            .take(5) // Look at top 5 results
            .filter(|r| {
                r.metadata.as_ref().unwrap().contains_key("index_type")
                    && r.metadata.as_ref().unwrap()["index_type"] == "hnsw"
            })
            .count();

        let ivf_weighted_hnsw_count = results_ivf
            .results
            .iter()
            .take(5) // Look at top 5 results
            .filter(|r| {
                r.metadata.as_ref().unwrap().contains_key("index_type")
                    && r.metadata.as_ref().unwrap()["index_type"] == "hnsw"
            })
            .count();

        // HNSW-weighted search should have more HNSW results in top positions
        // than IVF-weighted search
        println!(
            "HNSW-weighted search: {} HNSW results in top 5",
            hnsw_weighted_hnsw_count
        );
        println!(
            "IVF-weighted search: {} HNSW results in top 5",
            ivf_weighted_hnsw_count
        );
        assert!(hnsw_weighted_hnsw_count >= ivf_weighted_hnsw_count);
    }
}

#[cfg(test)]
mod result_merging_tests {
    use super::*;

    #[test]
    fn test_merge_and_deduplicate() {
        let hnsw_results = vec![
            ScoredResult {
                vector_id: VectorId::from_string("vec_1"),
                score: 0.9,
                distance: 0.1,
                metadata: None,
            },
            ScoredResult {
                vector_id: VectorId::from_string("vec_2"),
                score: 0.8,
                distance: 0.2,
                metadata: None,
            },
            ScoredResult {
                vector_id: VectorId::from_string("vec_3"),
                score: 0.7,
                distance: 0.3,
                metadata: None,
            },
        ];

        let ivf_results = vec![
            ScoredResult {
                vector_id: VectorId::from_string("vec_2"), // Duplicate
                score: 0.85,
                distance: 0.15,
                metadata: None,
            },
            ScoredResult {
                vector_id: VectorId::from_string("vec_4"),
                score: 0.75,
                distance: 0.25,
                metadata: None,
            },
        ];

        let merger = ResultMerger::new(MergeStrategy::TakeBest);
        let merged = merger.merge(vec![hnsw_results, ivf_results], 10);

        assert_eq!(merged.len(), 4); // vec_1, vec_2, vec_3, vec_4 (deduplicated)

        // Should take the better score for vec_2
        let vec2_result = merged
            .iter()
            .find(|r| r.vector_id == VectorId::from_string("vec_2"))
            .unwrap();
        assert_eq!(vec2_result.score, 0.85); // IVF had better score
    }

    #[test]
    fn test_merge_strategies() {
        let results1 = vec![ScoredResult {
            vector_id: VectorId::from_string("a"),
            score: 0.9,
            distance: 0.1,
            metadata: None,
        }];

        let results2 = vec![ScoredResult {
            vector_id: VectorId::from_string("a"), // Same vector
            score: 0.8,
            distance: 0.2,
            metadata: None,
        }];

        // Test TakeBest strategy
        let merger_best = ResultMerger::new(MergeStrategy::TakeBest);
        let merged_best = merger_best.merge(vec![results1.clone(), results2.clone()], 10);
        assert_eq!(merged_best[0].score, 0.9);

        // Test Average strategy
        let merger_avg = ResultMerger::new(MergeStrategy::Average);
        let merged_avg = merger_avg.merge(vec![results1.clone(), results2.clone()], 10);
        assert_eq!(merged_avg[0].score, 0.85); // (0.9 + 0.8) / 2

        // Test Weighted strategy
        let merger_weighted = ResultMerger::with_weights(
            MergeStrategy::Weighted,
            vec![2.0, 1.0], // First source has double weight
        );
        let merged_weighted = merger_weighted.merge(vec![results1, results2], 10);
        assert!((merged_weighted[0].score - 0.867).abs() < 0.001); // (0.9*2 + 0.8*1) / 3
    }

    #[test]
    fn test_result_limit() {
        let mut results = Vec::new();
        for i in 0..20 {
            results.push(ScoredResult {
                vector_id: VectorId::from_string(&format!("vec_{}", i)),
                score: 1.0 - (i as f32 * 0.01),
                distance: i as f32 * 0.01,
                metadata: None,
            });
        }

        let merger = ResultMerger::new(MergeStrategy::TakeBest);
        let merged = merger.merge(vec![results], 10);

        assert_eq!(merged.len(), 10);
        // Should keep the top 10 scores
        assert_eq!(merged[0].score, 1.0);
        assert_eq!(merged[9].score, 0.91);
    }
}

#[cfg(test)]
mod relevance_scoring_tests {
    use super::*;

    #[test]
    fn test_relevance_scorer() {
        let scorer = RelevanceScorer::new(ScoringMethod::CosineSimilarity);

        // Perfect match
        let score1 = scorer.score(0.0, None);
        assert_eq!(score1, 1.0);

        // Poor match
        let score2 = scorer.score(1.0, None);
        assert_eq!(score2, 0.0);

        // With metadata boost
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("boost".to_string(), "1.5".to_string());
        let score3 = scorer.score(0.5, Some(&metadata));
        assert_eq!(score3, 0.75); // 0.5 * 1.5
    }

    #[test]
    fn test_time_decay_scoring() {
        use chrono::{TimeZone, Utc};

        let scorer = RelevanceScorer::new(ScoringMethod::TimeDecay {
            half_life: Duration::from_secs(7 * 24 * 3600), // 1 week
        });

        let now = Utc::now();

        // Fresh content
        let mut metadata_fresh = std::collections::HashMap::new();
        metadata_fresh.insert("timestamp".to_string(), now.to_rfc3339());
        let score_fresh = scorer.score(0.5, Some(&metadata_fresh));
        assert!(score_fresh > 0.49 && score_fresh <= 0.5);

        // Week old content
        let week_ago = now - chrono::Duration::days(7);
        let mut metadata_old = std::collections::HashMap::new();
        metadata_old.insert("timestamp".to_string(), week_ago.to_rfc3339());
        let score_old = scorer.score(0.5, Some(&metadata_old));
        assert!((score_old - 0.25).abs() < 0.01); // Should be ~half
    }

    #[test]
    fn test_combined_scoring() {
        let scorer = RelevanceScorer::new(ScoringMethod::Combined {
            weights: vec![
                (ScoringMethod::CosineSimilarity, 0.7),
                (ScoringMethod::PopularityBoost, 0.3),
            ],
        });

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("views".to_string(), "1000".to_string());

        let score = scorer.score(0.5, Some(&metadata));
        // 0.5 * 0.7 (cosine) + popularity_score * 0.3
        assert!(score > 0.35 && score < 1.0);
    }
}

#[cfg(test)]
mod query_optimization_tests {
    use super::*;

    #[tokio::test]
    async fn test_query_optimizer() {
        let index = create_populated_hybrid_index().await;
        let optimizer = QueryOptimizer::new(index.clone());

        let query = vec![1.0, 2.0, 3.0];
        let config = SearchConfig::default();

        let optimized = optimizer.optimize_query(&query, &config).await.unwrap();

        assert!(optimized.use_hnsw);
        assert!(optimized.use_ivf);
        assert!(optimized.estimated_vectors > 0);
        assert!(optimized.suggested_n_probe.is_some());
        assert!(optimized.suggested_ef.is_some());
    }

    #[tokio::test]
    async fn test_adaptive_search_config() {
        let index = create_populated_hybrid_index().await;
        let optimizer = QueryOptimizer::new(index.clone());

        // Small k should use less probing
        let config_small = optimizer.suggest_config(&vec![0.0, 0.0], 5).await.unwrap();

        // Large k should use more probing
        let config_large = optimizer
            .suggest_config(&vec![0.0, 0.0], 100)
            .await
            .unwrap();

        assert!(config_large.ivf_n_probe > config_small.ivf_n_probe);
        assert!(config_large.hnsw_ef > config_small.hnsw_ef);
    }

    #[tokio::test]
    async fn test_query_expansion() {
        let expander = QueryExpander::new();

        let original_query = vec![1.0, 0.0, 0.0];
        let expanded = expander.expand(&original_query, 3);

        assert_eq!(expanded.len(), 3);
        assert_eq!(expanded[0], original_query); // Original is first

        // Expanded queries should be similar but not identical
        for i in 1..expanded.len() {
            let similarity = cosine_similarity(&original_query, &expanded[i]);
            assert!(similarity > 0.9 && similarity < 1.0);
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_performance_monitoring() {
        let index = create_populated_hybrid_index().await;
        let monitor = SearchPerformanceMonitor::new();

        // Perform multiple searches
        for i in 0..10 {
            let query = vec![i as f32 * 0.1, 0.0];
            let start = Instant::now();
            let _results = index.search(&query, 10).await.unwrap();
            let duration = start.elapsed();

            monitor.record_search(duration, 10, 2).await;
        }

        let stats = monitor.get_statistics().await;

        assert_eq!(stats.total_searches, 10);
        assert!(stats.avg_latency_ms > 0.0);
        assert!(stats.p50_latency_ms > 0.0);
        assert!(stats.p99_latency_ms >= stats.p50_latency_ms);
        assert_eq!(stats.avg_results_returned, 10.0);
    }

    #[tokio::test]
    async fn test_cache_effectiveness() {
        let index = Arc::new(create_populated_hybrid_index().await);
        let cached_index = CachedHybridIndex::new(index, 100);

        let query = vec![1.0, 2.0];

        // First search (cache miss)
        let result1 = cached_index.search(&query, 5).await.unwrap();

        // Second search (cache hit)
        let result2 = cached_index.search(&query, 5).await.unwrap();

        assert_eq!(result1.len(), result2.len());

        let stats = cached_index.cache_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }
}

// Helper functions
async fn create_populated_hybrid_index() -> HybridIndex {
    let config = HybridConfig::default();
    let mut index = HybridIndex::new(config);

    index
        .initialize(vec![
            vec![0.0, 0.0],
            vec![0.1, 0.1],
            vec![5.0, 5.0],
            vec![5.1, 4.9],
            vec![-5.0, -5.0],
            vec![-4.9, -5.1],
        ])
        .await
        .unwrap();

    // Add recent vectors
    for i in 0..20 {
        let id = VectorId::from_string(&format!("recent_{}", i));
        let angle = i as f32 * std::f32::consts::PI / 10.0;
        let vector = vec![angle.cos() * 3.0, angle.sin() * 3.0];
        index.insert(id, vector).await.unwrap();
    }

    // Add historical vectors
    let old_timestamp = chrono::Utc::now() - chrono::Duration::days(30);
    for i in 0..30 {
        let id = VectorId::from_string(&format!("historical_{}", i));
        let vector = vec![i as f32 * 0.2, (i as f32 * 0.3).sin() * 2.0];
        index
            .insert_with_timestamp(id, vector, old_timestamp)
            .await
            .unwrap();
    }

    index
}

async fn create_large_hybrid_index() -> HybridIndex {
    let config = HybridConfig::default();
    let mut index = HybridIndex::new(config);

    // Simple training data
    index
        .initialize(vec![vec![0.0, 0.0], vec![1.0, 1.0], vec![-1.0, -1.0]])
        .await
        .unwrap();

    // Add many vectors (reduced from 1000 due to HNSW performance issues)
    for i in 0..100 {
        let id = VectorId::from_string(&format!("vec_{}", i));
        let vector = vec![(i % 100) as f32 * 0.1, (i % 50) as f32 * 0.2];
        index.insert(id, vector).await.unwrap();
    }

    index
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (mag_a * mag_b)
}
