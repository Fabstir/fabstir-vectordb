use crate::core::types::{SearchResult, VectorId};
use crate::hybrid::core::{HybridError, HybridIndex, SearchConfig};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum SearchIntegrationError {
    #[error("Search timeout exceeded")]
    Timeout,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Hybrid error: {0}")]
    Hybrid(#[from] HybridError),
}

// Parallel search types
#[derive(Debug, Clone)]
pub struct ParallelSearchConfig {
    pub k: usize,
    pub timeout: Duration,
    pub max_concurrent_searches: usize,
    pub hnsw_weight: f32,
    pub ivf_weight: f32,
}

#[derive(Debug, Clone)]
pub struct ParallelSearchResult {
    pub results: Vec<ScoredResult>,
    pub search_time: Duration,
    pub indices_searched: usize,
    pub timed_out: bool,
}

#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub vector_id: VectorId,
    pub score: f32,
    pub distance: f32,
    pub metadata: Option<HashMap<String, String>>,
}

// Result merging types
#[derive(Debug, Clone)]
pub enum MergeStrategy {
    TakeBest,
    Average,
    Weighted,
}

pub struct ResultMerger {
    strategy: MergeStrategy,
    weights: Vec<f32>,
}

// Relevance scoring types
#[derive(Debug, Clone)]
pub enum ScoringMethod {
    CosineSimilarity,
    TimeDecay { half_life: Duration },
    PopularityBoost,
    Combined { weights: Vec<(ScoringMethod, f32)> },
}

pub struct RelevanceScorer {
    method: ScoringMethod,
}

// Query optimization types
#[derive(Debug, Clone)]
pub struct OptimizedQuery {
    pub use_hnsw: bool,
    pub use_ivf: bool,
    pub estimated_vectors: usize,
    pub suggested_n_probe: Option<usize>,
    pub suggested_ef: Option<usize>,
}

pub struct QueryOptimizer {
    index: HybridIndex,
}

pub struct QueryExpander;

// Performance monitoring types
#[derive(Debug, Clone)]
pub struct SearchStatistics {
    pub total_searches: usize,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_results_returned: f64,
}

pub struct SearchPerformanceMonitor {
    stats: Arc<RwLock<Vec<(Duration, usize, usize)>>>,
}

// Caching types
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub hit_rate: f64,
}

pub struct CachedHybridIndex {
    index: Arc<HybridIndex>,
    cache: Arc<RwLock<HashMap<(Vec<u8>, usize), Vec<SearchResult>>>>,
    max_cache_size: usize,
    stats: Arc<RwLock<(usize, usize)>>,
}

// Implementations

impl HybridIndex {
    pub async fn parallel_search(
        &self,
        query: &[f32],
        config: ParallelSearchConfig,
    ) -> Result<ParallelSearchResult, SearchIntegrationError> {
        let start = Instant::now();

        // Create search configs for each index
        let mut search_config = SearchConfig::default();
        search_config.k = config.k;

        let timeout_result = tokio::time::timeout(config.timeout, async {
            // Search recent index
            let mut recent_config = search_config.clone();
            recent_config.search_historical = false;
            let recent_future = self.search_with_config(query, recent_config);

            // Search historical index
            let mut historical_config = search_config.clone();
            historical_config.search_recent = false;
            let historical_future = self.search_with_config(query, historical_config);

            // Wait for both searches concurrently
            let (recent_results, historical_results) =
                tokio::join!(recent_future, historical_future);

            let mut all_results = Vec::new();
            let mut indices_searched = 0;

            // Process recent results
            if let Ok(results) = recent_results {
                for result in results {
                    let mut metadata = HashMap::new();
                    metadata.insert("index_type".to_string(), "hnsw".to_string());

                    all_results.push(ScoredResult {
                        vector_id: result.vector_id,
                        score: (1.0 - result.distance) * config.hnsw_weight,
                        distance: result.distance,
                        metadata: Some(metadata),
                    });
                }
                indices_searched += 1;
            }

            // Process historical results
            if let Ok(results) = historical_results {
                for result in results {
                    let mut metadata = HashMap::new();
                    metadata.insert("index_type".to_string(), "ivf".to_string());

                    all_results.push(ScoredResult {
                        vector_id: result.vector_id,
                        score: (1.0 - result.distance) * config.ivf_weight,
                        distance: result.distance,
                        metadata: Some(metadata),
                    });
                }
                indices_searched += 1;
            }

            // Sort by score and limit
            all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            all_results.truncate(config.k);

            (all_results, indices_searched)
        })
        .await;

        match timeout_result {
            Ok((results, indices_searched)) => Ok(ParallelSearchResult {
                results,
                search_time: start.elapsed(),
                indices_searched,
                timed_out: false,
            }),
            Err(_) => {
                // Timeout occurred
                Ok(ParallelSearchResult {
                    results: Vec::new(),
                    search_time: start.elapsed(),
                    indices_searched: 0,
                    timed_out: true,
                })
            }
        }
    }
}

impl ResultMerger {
    pub fn new(strategy: MergeStrategy) -> Self {
        Self {
            strategy,
            weights: vec![],
        }
    }

    pub fn with_weights(strategy: MergeStrategy, weights: Vec<f32>) -> Self {
        Self { strategy, weights }
    }

    pub fn merge(&self, result_sets: Vec<Vec<ScoredResult>>, k: usize) -> Vec<ScoredResult> {
        let mut merged_map: HashMap<VectorId, Vec<(ScoredResult, usize)>> = HashMap::new();

        // Group results by vector ID
        for (source_idx, results) in result_sets.iter().enumerate() {
            for result in results {
                merged_map
                    .entry(result.vector_id.clone())
                    .or_insert_with(Vec::new)
                    .push((result.clone(), source_idx));
            }
        }

        // Apply merge strategy
        let mut final_results = Vec::new();
        for (vector_id, occurrences) in merged_map {
            let merged_result = match &self.strategy {
                MergeStrategy::TakeBest => {
                    // Take the result with the highest score
                    occurrences
                        .into_iter()
                        .max_by(|a, b| a.0.score.partial_cmp(&b.0.score).unwrap())
                        .map(|(r, _)| r)
                        .unwrap()
                }
                MergeStrategy::Average => {
                    // Average the scores
                    let sum: f32 = occurrences.iter().map(|(r, _)| r.score).sum();
                    let avg_score = sum / occurrences.len() as f32;
                    let avg_distance: f32 =
                        occurrences.iter().map(|(r, _)| r.distance).sum::<f32>()
                            / occurrences.len() as f32;

                    ScoredResult {
                        vector_id,
                        score: avg_score,
                        distance: avg_distance,
                        metadata: occurrences[0].0.metadata.clone(),
                    }
                }
                MergeStrategy::Weighted => {
                    // Weighted average based on source weights
                    let mut weighted_sum = 0.0;
                    let mut weight_sum = 0.0;
                    let mut weighted_distance = 0.0;

                    for (result, source_idx) in &occurrences {
                        let weight = self.weights.get(*source_idx).copied().unwrap_or(1.0);
                        weighted_sum += result.score * weight;
                        weighted_distance += result.distance * weight;
                        weight_sum += weight;
                    }

                    ScoredResult {
                        vector_id,
                        score: weighted_sum / weight_sum,
                        distance: weighted_distance / weight_sum,
                        metadata: occurrences[0].0.metadata.clone(),
                    }
                }
            };

            final_results.push(merged_result);
        }

        // Sort by score and limit
        final_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        final_results.truncate(k);
        final_results
    }
}

impl RelevanceScorer {
    pub fn new(method: ScoringMethod) -> Self {
        Self { method }
    }

    pub fn score(&self, distance: f32, metadata: Option<&HashMap<String, String>>) -> f32 {
        match &self.method {
            ScoringMethod::CosineSimilarity => {
                let base_score = 1.0 - distance;

                // Apply boost if present in metadata
                if let Some(meta) = metadata {
                    if let Some(boost_str) = meta.get("boost") {
                        if let Ok(boost) = boost_str.parse::<f32>() {
                            return base_score * boost;
                        }
                    }
                }

                base_score
            }
            ScoringMethod::TimeDecay { half_life } => {
                let base_score = 1.0 - distance;

                if let Some(meta) = metadata {
                    if let Some(timestamp_str) = meta.get("timestamp") {
                        if let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp_str) {
                            let now = Utc::now();
                            let age = now
                                .signed_duration_since(timestamp.with_timezone(&Utc))
                                .to_std()
                                .unwrap_or(Duration::from_secs(0));

                            let decay_factor =
                                0.5_f32.powf(age.as_secs_f32() / half_life.as_secs_f32());
                            return base_score * decay_factor;
                        }
                    }
                }

                base_score
            }
            ScoringMethod::PopularityBoost => {
                let base_score = 1.0 - distance;

                if let Some(meta) = metadata {
                    if let Some(views_str) = meta.get("views") {
                        if let Ok(views) = views_str.parse::<f32>() {
                            // Logarithmic boost based on views
                            let boost = (1.0 + views).ln() / 10.0;
                            return base_score * (1.0 + boost);
                        }
                    }
                }

                base_score
            }
            ScoringMethod::Combined { weights } => {
                let mut combined_score = 0.0;
                let mut total_weight = 0.0;

                for (method, weight) in weights {
                    let scorer = RelevanceScorer::new(method.clone());
                    combined_score += scorer.score(distance, metadata) * weight;
                    total_weight += weight;
                }

                if total_weight > 0.0 {
                    combined_score / total_weight
                } else {
                    1.0 - distance
                }
            }
        }
    }
}

impl QueryOptimizer {
    pub fn new(index: HybridIndex) -> Self {
        Self { index }
    }

    pub async fn optimize_query(
        &self,
        _query: &[f32],
        _config: &SearchConfig,
    ) -> Result<OptimizedQuery, SearchIntegrationError> {
        let stats = self.index.get_statistics().await;

        // Determine which indices to use based on data distribution
        let use_hnsw = stats.recent_vectors > 0;
        let use_ivf = stats.historical_vectors > 0;

        // Suggest search parameters based on dataset size
        let total_vectors = stats.total_vectors;
        let suggested_n_probe = if total_vectors < 1000 {
            Some(5)
        } else if total_vectors < 10000 {
            Some(10)
        } else {
            Some(20)
        };

        let suggested_ef = if total_vectors < 1000 {
            Some(50)
        } else if total_vectors < 10000 {
            Some(100)
        } else {
            Some(200)
        };

        Ok(OptimizedQuery {
            use_hnsw,
            use_ivf,
            estimated_vectors: total_vectors,
            suggested_n_probe,
            suggested_ef,
        })
    }

    pub async fn suggest_config(
        &self,
        _query: &[f32],
        k: usize,
    ) -> Result<SearchConfig, SearchIntegrationError> {
        let stats = self.index.get_statistics().await;

        let mut config = SearchConfig::default();
        config.k = k;

        // Adjust parameters based on k and dataset size
        if k < 10 {
            config.ivf_n_probe = 5;
            config.hnsw_ef = 50;
        } else if k < 50 {
            config.ivf_n_probe = 10;
            config.hnsw_ef = 100;
        } else {
            config.ivf_n_probe = 20;
            config.hnsw_ef = 200;
        }

        // Adjust based on data distribution
        if stats.recent_vectors == 0 {
            config.search_recent = false;
        }
        if stats.historical_vectors == 0 {
            config.search_historical = false;
        }

        Ok(config)
    }
}

impl QueryExpander {
    pub fn new() -> Self {
        Self
    }

    pub fn expand(&self, query: &[f32], num_expansions: usize) -> Vec<Vec<f32>> {
        use rand::{thread_rng, Rng};

        let mut expanded = vec![query.to_vec()];
        let mut rng = thread_rng();

        // Generate variations by adding small noise
        for _ in 1..num_expansions {
            let mut variation = query.to_vec();
            for val in &mut variation {
                *val += rng.gen_range(-0.05..0.05);
            }

            // Normalize to maintain magnitude
            let magnitude: f32 = variation.iter().map(|x| x * x).sum::<f32>().sqrt();
            if magnitude > 0.0 {
                for val in &mut variation {
                    *val /= magnitude;
                }

                // Scale back to approximate original magnitude
                let orig_magnitude: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
                for val in &mut variation {
                    *val *= orig_magnitude;
                }
            }

            expanded.push(variation);
        }

        expanded
    }
}

impl SearchPerformanceMonitor {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn record_search(
        &self,
        latency: Duration,
        results_returned: usize,
        indices_searched: usize,
    ) {
        let mut stats = self.stats.write().await;
        stats.push((latency, results_returned, indices_searched));
    }

    pub async fn get_statistics(&self) -> SearchStatistics {
        let stats = self.stats.read().await;

        if stats.is_empty() {
            return SearchStatistics {
                total_searches: 0,
                avg_latency_ms: 0.0,
                p50_latency_ms: 0.0,
                p99_latency_ms: 0.0,
                avg_results_returned: 0.0,
            };
        }

        let total_searches = stats.len();

        // Calculate average latency
        let total_latency: Duration = stats.iter().map(|(d, _, _)| *d).sum();
        let avg_latency_ms = total_latency.as_secs_f64() * 1000.0 / total_searches as f64;

        // Calculate percentiles
        let mut latencies: Vec<f64> = stats
            .iter()
            .map(|(d, _, _)| d.as_secs_f64() * 1000.0)
            .collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p50_idx = (total_searches as f64 * 0.5) as usize;
        let p99_idx = ((total_searches as f64 * 0.99) as usize).min(total_searches - 1);

        let p50_latency_ms = latencies[p50_idx];
        let p99_latency_ms = latencies[p99_idx];

        // Calculate average results returned
        let total_results: usize = stats.iter().map(|(_, r, _)| *r).sum();
        let avg_results_returned = total_results as f64 / total_searches as f64;

        SearchStatistics {
            total_searches,
            avg_latency_ms,
            p50_latency_ms,
            p99_latency_ms,
            avg_results_returned,
        }
    }
}

impl CachedHybridIndex {
    pub fn new(index: Arc<HybridIndex>, max_cache_size: usize) -> Self {
        Self {
            index,
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size,
            stats: Arc::new(RwLock::new((0, 0))), // (hits, misses)
        }
    }

    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, HybridError> {
        // Create cache key from query and k
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        for val in query {
            val.to_bits().hash(&mut hasher);
        }
        k.hash(&mut hasher);
        let key = (hasher.finish().to_le_bytes().to_vec(), k);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(results) = cache.get(&key) {
                let mut stats = self.stats.write().await;
                stats.0 += 1; // Increment hits
                return Ok(results.clone());
            }
        }

        // Cache miss - perform search
        let mut stats = self.stats.write().await;
        stats.1 += 1; // Increment misses
        drop(stats);

        let results = self.index.search(query, k).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;

            // Evict if cache is full (simple FIFO)
            if cache.len() >= self.max_cache_size {
                if let Some(first_key) = cache.keys().next().cloned() {
                    cache.remove(&first_key);
                }
            }

            cache.insert(key, results.clone());
        }

        Ok(results)
    }

    pub async fn cache_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        let hits = stats.0;
        let misses = stats.1;
        let total = hits + misses;

        CacheStats {
            hits,
            misses,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}
