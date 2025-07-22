use crate::core::types::VectorId;
use crate::hnsw::core::{HNSWError, HNSWIndex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("HNSW error: {0}")]
    HNSWError(#[from] HNSWError),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

#[derive(Debug, Clone)]
pub struct BatchInsertResult {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<(VectorId, HNSWError)>,
}

#[derive(Debug, Clone)]
pub struct BatchDeleteResult {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<(VectorId, HNSWError)>,
}

#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub edges_added: usize,
    pub edges_removed: usize,
}

#[derive(Debug, Clone)]
pub struct RebalanceResult {
    pub nodes_moved: usize,
    pub layers_adjusted: usize,
}

#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub layers_removed: usize,
    pub nodes_relocated: usize,
}

#[derive(Debug, Clone)]
pub struct DefragmentResult {
    pub bytes_saved: usize,
    pub nodes_moved: usize,
}

#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub avg_degree: f32,
    pub max_layer: usize,
    pub connected_components: usize,
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub total_bytes: usize,
    pub nodes_bytes: usize,
    pub vectors_bytes: usize,
    pub graph_bytes: usize,
}

impl HNSWIndex {
    // Batch operations
    pub fn batch_insert(
        &mut self,
        batch: Vec<(VectorId, Vec<f32>)>,
    ) -> Result<BatchInsertResult, OperationError> {
        let mut result = BatchInsertResult {
            successful: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for (id, vector) in batch {
            match self.insert(id.clone(), vector) {
                Ok(_) => result.successful += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push((id, e));
                }
            }
        }

        Ok(result)
    }

    pub fn batch_insert_with_progress<F>(
        &mut self,
        batch: Vec<(VectorId, Vec<f32>)>,
        mut progress: F,
    ) -> Result<BatchInsertResult, OperationError>
    where
        F: FnMut(usize, usize),
    {
        let total = batch.len();
        let mut result = BatchInsertResult {
            successful: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for (i, (id, vector)) in batch.into_iter().enumerate() {
            match self.insert(id.clone(), vector) {
                Ok(_) => result.successful += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push((id, e));
                }
            }
            progress(i + 1, total);
        }

        Ok(result)
    }

    // Deletion operations
    pub fn mark_deleted(&mut self, id: &VectorId) -> Result<(), HNSWError> {
        let mut nodes = self.nodes().write().unwrap();
        match nodes.get_mut(id) {
            Some(node) => {
                node.mark_deleted();
                Ok(())
            }
            None => Err(HNSWError::VectorNotFound(id.clone())),
        }
    }

    pub fn is_deleted(&self, id: &VectorId) -> bool {
        self.nodes()
            .read()
            .unwrap()
            .get(id)
            .map(|node| node.is_deleted())
            .unwrap_or(false)
    }

    pub fn batch_delete(&mut self, ids: &[VectorId]) -> Result<BatchDeleteResult, OperationError> {
        let mut result = BatchDeleteResult {
            successful: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for id in ids {
            match self.mark_deleted(id) {
                Ok(_) => result.successful += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push((id.clone(), e));
                }
            }
        }

        Ok(result)
    }

    pub fn active_count(&self) -> usize {
        self.nodes()
            .read()
            .unwrap()
            .values()
            .filter(|node| !node.is_deleted())
            .count()
    }

    pub fn vacuum(&mut self) -> Result<usize, OperationError> {
        let mut nodes = self.nodes().write().unwrap();
        let deleted_ids: Vec<_> = nodes
            .iter()
            .filter(|(_, node)| node.is_deleted())
            .map(|(id, _)| id.clone())
            .collect();

        let removed_count = deleted_ids.len();

        // Remove deleted nodes
        for id in &deleted_ids {
            nodes.remove(id);
        }

        // Clean up references to deleted nodes from remaining nodes
        for node in nodes.values_mut() {
            for layer in 0..=node.level() {
                let neighbors = node.neighbors_mut(layer);
                neighbors.retain(|neighbor_id| !deleted_ids.contains(neighbor_id));
            }
        }

        Ok(removed_count)
    }

    // Maintenance operations
    pub fn optimize_connections(
        &mut self,
        _threshold: f32,
    ) -> Result<OptimizationResult, OperationError> {
        let result = OptimizationResult {
            edges_added: 0,
            edges_removed: 0,
        };

        // TODO: Implement connection optimization
        // For now, return a placeholder result
        Ok(result)
    }

    pub fn rebalance(&mut self) -> Result<RebalanceResult, OperationError> {
        let result = RebalanceResult {
            nodes_moved: 0,
            layers_adjusted: 0,
        };

        // TODO: Implement graph rebalancing
        Ok(result)
    }

    pub fn get_graph_stats(&self) -> GraphStats {
        let nodes = self.nodes().read().unwrap();
        let active_nodes: Vec<_> = nodes.values().filter(|node| !node.is_deleted()).collect();

        let total_nodes = active_nodes.len();

        if total_nodes == 0 {
            return GraphStats {
                total_nodes: 0,
                total_edges: 0,
                avg_degree: 0.0,
                max_layer: 0,
                connected_components: 0,
            };
        }

        let mut total_edges = 0;
        let mut max_layer = 0;

        for node in &active_nodes {
            max_layer = max_layer.max(node.level());
            for layer in 0..=node.level() {
                total_edges += node.neighbors(layer).len();
            }
        }

        // Since edges are bidirectional, divide by 2
        total_edges /= 2;

        let avg_degree = if total_nodes > 0 {
            (total_edges * 2) as f32 / total_nodes as f32
        } else {
            0.0
        };

        // Simple connectivity check - if we have nodes and they all have neighbors, assume 1 component
        let connected_components = if total_nodes > 0 { 1 } else { 0 };

        GraphStats {
            total_nodes,
            total_edges,
            avg_degree,
            max_layer,
            connected_components,
        }
    }

    pub fn estimate_memory_usage(&self) -> MemoryUsage {
        let nodes = self.nodes().read().unwrap();

        let mut nodes_bytes = 0;
        let mut vectors_bytes = 0;
        let mut graph_bytes = 0;

        for node in nodes.values() {
            // Node overhead
            nodes_bytes += std::mem::size_of::<VectorId>()
                + std::mem::size_of::<bool>()
                + std::mem::size_of::<usize>();

            // Vector storage
            vectors_bytes += node.vector().len() * std::mem::size_of::<f32>();

            // Graph connections
            for layer in 0..=node.level() {
                graph_bytes += node.neighbors(layer).len() * std::mem::size_of::<VectorId>();
            }
        }

        let total_bytes = std::mem::size_of::<Self>() + nodes_bytes + vectors_bytes + graph_bytes;

        MemoryUsage {
            total_bytes,
            nodes_bytes,
            vectors_bytes,
            graph_bytes,
        }
    }

    // Compaction operations
    pub fn compact_layers(&mut self) -> Result<CompactionResult, OperationError> {
        let result = CompactionResult {
            layers_removed: 0,
            nodes_relocated: 0,
        };

        // TODO: Implement layer compaction
        Ok(result)
    }

    pub fn defragment(&mut self) -> Result<DefragmentResult, OperationError> {
        let result = DefragmentResult {
            bytes_saved: 0,
            nodes_moved: 0,
        };

        // TODO: Implement defragmentation
        Ok(result)
    }
}
