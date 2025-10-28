use crate::core::types::{SearchResult, VectorId};
use crate::ivf::core::{Centroid, ClusterId, IVFConfig, IVFError, IVFIndex};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("IVF error: {0}")]
    IVF(#[from] IVFError),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

// Batch operation results
#[derive(Debug, Clone)]
pub struct BatchInsertResult {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<(VectorId, IVFError)>,
}

// Retraining results
#[derive(Debug, Clone)]
pub struct RetrainResult {
    pub old_clusters: usize,
    pub new_clusters: usize,
    pub vectors_reassigned: usize,
    pub converged: bool,
}

#[derive(Debug, Clone)]
pub struct AddClustersResult {
    pub clusters_added: usize,
    pub vectors_reassigned: usize,
}

#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub iterations: usize,
    pub improvement: f32,
}

// Statistics structures
#[derive(Debug, Clone)]
pub struct ClusterStats {
    pub n_clusters: usize,
    pub total_vectors: usize,
    pub avg_cluster_size: f32,
    pub size_variance: f32,
    pub empty_clusters: usize,
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub total_bytes: usize,
    pub centroids_bytes: usize,
    pub vectors_bytes: usize,
    pub inverted_lists_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct SearchQuality {
    pub avg_recall: f32,
    pub avg_precision: f32,
    pub avg_query_time_ms: f32,
    pub queries_evaluated: usize,
}

// Maintenance results
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub bytes_saved: usize,
    pub clusters_compacted: usize,
}

#[derive(Debug, Clone)]
pub struct BalanceResult {
    pub vectors_moved: usize,
    pub balance_improved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedCentroid {
    pub id: usize,
    pub vector: Vec<f32>,
    pub dimension: usize,
}

// Extension trait for IVFIndex
impl IVFIndex {
    // Batch operations
    pub fn batch_insert(
        &mut self,
        batch: Vec<(VectorId, Vec<f32>)>,
    ) -> Result<BatchInsertResult, OperationError> {
        let mut successful = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for (id, vector) in batch {
            match self.insert(id.clone(), vector) {
                Ok(_) => successful += 1,
                Err(e) => {
                    failed += 1;
                    errors.push((id, e));
                }
            }
        }

        Ok(BatchInsertResult {
            successful,
            failed,
            errors,
        })
    }

    pub async fn batch_search(
        &self,
        queries: &[Vec<f32>],
        k: usize,
    ) -> Result<Vec<Vec<SearchResult>>, OperationError> {
        let mut results = Vec::new();

        for query in queries {
            let query_results = self.search(query, k).await?;
            results.push(query_results);
        }

        Ok(results)
    }

    // Retraining operations
    pub fn retrain(&mut self, new_config: IVFConfig) -> Result<RetrainResult, OperationError> {
        if !self.is_trained() {
            return Err(IVFError::NotTrained.into());
        }

        let old_clusters = self.config().n_clusters;
        let old_vectors = self.total_vectors();

        // Collect all existing vectors
        let mut all_vectors = Vec::new();
        let mut all_ids = Vec::new();

        for list in self.get_all_inverted_lists().values() {
            for (id, vector) in &list.vectors {
                all_ids.push(id.clone());
                all_vectors.push(vector.clone());
            }
        }

        // Update config
        self.config = new_config;
        self.trained = false;

        // Retrain with new config
        let train_result = self.train(&all_vectors)?;

        // Clear inverted lists and reset counter
        self.inverted_lists.clear();
        self.total_vectors = 0;
        for i in 0..self.config.n_clusters {
            self.inverted_lists
                .insert(ClusterId(i), crate::ivf::core::InvertedList::new());
        }

        // Reinsert all vectors
        for (id, vector) in all_ids.iter().zip(all_vectors.iter()) {
            self.insert(id.clone(), vector.clone())?;
        }

        Ok(RetrainResult {
            old_clusters,
            new_clusters: self.config.n_clusters,
            vectors_reassigned: old_vectors,
            converged: train_result.converged,
        })
    }

    pub fn add_clusters(
        &mut self,
        n_clusters_to_add: usize,
    ) -> Result<AddClustersResult, OperationError> {
        if !self.is_trained() {
            return Err(IVFError::NotTrained.into());
        }

        if n_clusters_to_add == 0 {
            return Err(OperationError::InvalidParameter(
                "Cannot add 0 clusters".to_string(),
            ));
        }

        // Create new config with more clusters
        let mut new_config = self.config.clone();
        new_config.n_clusters += n_clusters_to_add;

        // Retrain with new config
        let retrain_result = self.retrain(new_config)?;

        Ok(AddClustersResult {
            clusters_added: n_clusters_to_add,
            vectors_reassigned: retrain_result.vectors_reassigned,
        })
    }

    pub fn optimize_clusters(&mut self) -> Result<OptimizationResult, OperationError> {
        if !self.is_trained() {
            return Err(IVFError::NotTrained.into());
        }

        let initial_variance = self.calculate_size_variance();

        // Collect all vectors
        let mut all_vectors = Vec::new();
        let mut all_ids = Vec::new();

        for list in self.get_all_inverted_lists().values() {
            for (id, vector) in &list.vectors {
                all_ids.push(id.clone());
                all_vectors.push(vector.clone());
            }
        }

        // Retrain with same config but fresh centroids
        let train_result = self.train(&all_vectors)?;

        // Clear and reinsert
        self.inverted_lists.clear();
        for i in 0..self.config.n_clusters {
            self.inverted_lists
                .insert(ClusterId(i), crate::ivf::core::InvertedList::new());
        }

        for (id, vector) in all_ids.iter().zip(all_vectors.iter()) {
            self.insert(id.clone(), vector.clone())?;
        }

        let final_variance = self.calculate_size_variance();
        let improvement = (initial_variance - final_variance).max(0.0);

        Ok(OptimizationResult {
            iterations: train_result.iterations,
            improvement,
        })
    }

    // Statistics operations
    pub fn get_cluster_stats(&self) -> ClusterStats {
        let distribution = self.get_cluster_distribution();
        let n_clusters = self.config.n_clusters;
        let total_vectors = self.total_vectors();

        let sizes: Vec<f32> = (0..n_clusters)
            .map(|i| distribution.get(&ClusterId(i)).copied().unwrap_or(0) as f32)
            .collect();

        let avg_cluster_size = if n_clusters > 0 {
            total_vectors as f32 / n_clusters as f32
        } else {
            0.0
        };

        let size_variance = self.calculate_size_variance();
        let empty_clusters = sizes.iter().filter(|&&s| s == 0.0).count();

        ClusterStats {
            n_clusters,
            total_vectors,
            avg_cluster_size,
            size_variance,
            empty_clusters,
        }
    }

    pub fn estimate_memory_usage(&self) -> MemoryUsage {
        let dim = self.dimension().unwrap_or(0);

        // Centroids: n_clusters * dimension * 4 bytes per f32
        let centroids_bytes = self.centroids.len() * dim * 4;

        // Vectors and inverted lists
        let mut vectors_bytes = 0;
        let mut inverted_lists_bytes = 0;

        for list in self.inverted_lists.values() {
            // HashMap overhead per list
            inverted_lists_bytes += 48; // Approximate HashMap base size

            for (id, vector) in &list.vectors {
                // VectorId size (approximate)
                inverted_lists_bytes += 48; // Approximate VectorId size
                                            // Vector data
                vectors_bytes += vector.len() * 4;
                // HashMap entry overhead
                inverted_lists_bytes += 32;
            }
        }

        // Index structure overhead
        let structure_overhead = 256; // Approximate

        MemoryUsage {
            total_bytes: centroids_bytes
                + vectors_bytes
                + inverted_lists_bytes
                + structure_overhead,
            centroids_bytes,
            vectors_bytes,
            inverted_lists_bytes,
        }
    }

    pub async fn evaluate_search_quality(
        &self,
        test_queries: &[Vec<f32>],
        k: usize,
    ) -> Result<SearchQuality, OperationError> {
        if test_queries.is_empty() {
            return Err(OperationError::InvalidParameter(
                "No test queries provided".to_string(),
            ));
        }

        let mut total_recall = 0.0;
        let mut total_precision = 0.0;
        let mut total_time_ms = 0.0;

        for query in test_queries {
            let start = Instant::now();

            // Search with current n_probe
            let results = self.search(query, k).await?;

            // Search with all clusters for ground truth
            let ground_truth = self.search_with_config(query, k, self.config.n_clusters).await?;

            let elapsed = start.elapsed();
            total_time_ms += elapsed.as_secs_f32() * 1000.0;

            // Calculate recall and precision
            let result_ids: Vec<_> = results.iter().map(|r| &r.vector_id).collect();
            let truth_ids: Vec<_> = ground_truth.iter().map(|r| &r.vector_id).collect();

            let mut matches = 0;
            for id in &result_ids {
                if truth_ids.contains(id) {
                    matches += 1;
                }
            }

            let recall = if truth_ids.is_empty() {
                1.0
            } else {
                matches as f32 / truth_ids.len().min(k) as f32
            };

            let precision = if result_ids.is_empty() {
                0.0
            } else {
                matches as f32 / result_ids.len() as f32
            };

            total_recall += recall;
            total_precision += precision;
        }

        let queries_evaluated = test_queries.len();

        Ok(SearchQuality {
            avg_recall: total_recall / queries_evaluated as f32,
            avg_precision: total_precision / queries_evaluated as f32,
            avg_query_time_ms: total_time_ms / queries_evaluated as f32,
            queries_evaluated,
        })
    }

    // Maintenance operations
    pub fn compact_clusters(&mut self) -> Result<CompactionResult, OperationError> {
        let before_memory = self.estimate_memory_usage();
        let mut clusters_compacted = 0;

        // In a real implementation, we would:
        // 1. Remove deleted vectors
        // 2. Shrink capacity of HashMaps
        // 3. Reorganize memory layout

        // For now, just shrink to fit
        for list in self.inverted_lists.values_mut() {
            list.vectors.shrink_to_fit();
            clusters_compacted += 1;
        }

        let after_memory = self.estimate_memory_usage();
        let bytes_saved = if before_memory.total_bytes > after_memory.total_bytes {
            before_memory.total_bytes - after_memory.total_bytes
        } else {
            0
        };

        Ok(CompactionResult {
            bytes_saved,
            clusters_compacted,
        })
    }

    pub fn balance_clusters(&mut self, threshold: f32) -> Result<BalanceResult, OperationError> {
        if threshold <= 0.0 || threshold >= 1.0 {
            return Err(OperationError::InvalidParameter(
                "Threshold must be between 0 and 1".to_string(),
            ));
        }

        let stats = self.get_cluster_stats();
        let target_size = stats.avg_cluster_size;
        let max_deviation = target_size * threshold;

        let mut vectors_moved = 0;
        let initial_variance = stats.size_variance;

        // Collect vectors from oversized and undersized clusters info
        let mut cluster_sizes: Vec<(ClusterId, usize)> = self
            .inverted_lists
            .iter()
            .map(|(id, list)| (*id, list.vectors.len()))
            .collect();

        // Sort by size descending
        cluster_sizes.sort_by(|a, b| b.1.cmp(&a.1));

        let mut oversized_vectors = Vec::new();

        // Take vectors from the most oversized clusters
        for (cluster_id, size) in &cluster_sizes {
            let size_f32 = *size as f32;
            if size_f32 > target_size + max_deviation && *size > 0 {
                if let Some(list) = self.inverted_lists.get(cluster_id) {
                    // Take some vectors from oversized clusters
                    let excess = ((size_f32 - target_size).max(1.0)) as usize;
                    let to_take = excess.min(*size / 2); // Don't take more than half

                    for (id, vector) in list.vectors.iter().take(to_take) {
                        oversized_vectors.push((id.clone(), vector.clone(), *cluster_id));
                    }
                }
            }
        }

        // Remove from original clusters and reassign
        for (id, vector, old_cluster) in oversized_vectors {
            if let Some(list) = self.inverted_lists.get_mut(&old_cluster) {
                list.vectors.remove(&id);
            }

            // Find new best cluster
            let new_cluster = self.find_nearest_centroid(&vector);
            if new_cluster != old_cluster {
                if let Some(list) = self.inverted_lists.get_mut(&new_cluster) {
                    list.vectors.insert(id, vector);
                    vectors_moved += 1;
                }
            } else {
                // Put it back
                if let Some(list) = self.inverted_lists.get_mut(&old_cluster) {
                    list.vectors.insert(id, vector);
                }
            }
        }

        let final_variance = self.calculate_size_variance();
        let balance_improved = final_variance < initial_variance;

        Ok(BalanceResult {
            vectors_moved,
            balance_improved,
        })
    }

    pub fn export_centroids(&self) -> Result<Vec<ExportedCentroid>, OperationError> {
        if !self.is_trained() {
            return Err(IVFError::NotTrained.into());
        }

        let centroids = self
            .centroids
            .iter()
            .map(|c| ExportedCentroid {
                id: c.id().0,
                vector: c.vector().clone(),
                dimension: c.dimension(),
            })
            .collect();

        Ok(centroids)
    }

    pub fn import_centroids(
        &mut self,
        centroids_data: Vec<ExportedCentroid>,
    ) -> Result<(), OperationError> {
        if centroids_data.is_empty() {
            return Err(OperationError::InvalidParameter(
                "No centroids to import".to_string(),
            ));
        }

        // Validate dimensions are consistent
        let dimension = centroids_data[0].dimension;
        for centroid in &centroids_data {
            if centroid.dimension != dimension {
                return Err(OperationError::InvalidParameter(
                    "Inconsistent centroid dimensions".to_string(),
                ));
            }
        }

        // Import centroids
        self.centroids = centroids_data
            .into_iter()
            .map(|c| Centroid::new(ClusterId(c.id), c.vector))
            .collect();

        self.dimension = Some(dimension);
        self.trained = true;

        // Initialize empty inverted lists
        self.inverted_lists.clear();
        for i in 0..self.centroids.len() {
            self.inverted_lists
                .insert(ClusterId(i), crate::ivf::core::InvertedList::new());
        }

        Ok(())
    }

    // Helper method
    fn calculate_size_variance(&self) -> f32 {
        let distribution = self.get_cluster_distribution();
        let n_clusters = self.config.n_clusters;

        let sizes: Vec<f32> = (0..n_clusters)
            .map(|i| distribution.get(&ClusterId(i)).copied().unwrap_or(0) as f32)
            .collect();

        let mean = sizes.iter().sum::<f32>() / n_clusters as f32;
        let variance = sizes.iter().map(|&s| (s - mean).powi(2)).sum::<f32>() / n_clusters as f32;

        variance
    }
}
