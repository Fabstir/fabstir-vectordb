use crate::core::types::{SearchResult, VectorId};
use crate::storage::chunk_loader::ChunkLoader;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum HNSWError {
    #[error("Vector with ID {0:?} already exists")]
    DuplicateVector(VectorId),

    #[error("Vector not found: {0:?}")]
    VectorNotFound(VectorId),

    #[error("Invalid dimension: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Chunk loading error: {0}")]
    ChunkLoadError(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HNSWConfig {
    pub max_connections: usize,
    pub max_connections_layer_0: usize,
    pub ef_construction: usize,
    pub seed: Option<u64>,
}

impl Default for HNSWConfig {
    fn default() -> Self {
        Self {
            max_connections: 16,
            max_connections_layer_0: 32,
            ef_construction: 200,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HNSWNode {
    id: VectorId,
    vector: Vec<f32>,
    level: usize,
    neighbors: Vec<HashSet<VectorId>>, // neighbors[i] = neighbors at layer i
    #[serde(default)]
    is_deleted: bool,
}

impl HNSWNode {
    pub fn new(id: VectorId, vector: Vec<f32>) -> Self {
        Self {
            id,
            vector,
            level: 0,
            neighbors: vec![HashSet::new()],
            is_deleted: false,
        }
    }

    pub fn id(&self) -> &VectorId {
        &self.id
    }

    pub fn vector(&self) -> &Vec<f32> {
        &self.vector
    }

    pub fn level(&self) -> usize {
        self.level
    }

    pub fn neighbors(&self, layer: usize) -> &HashSet<VectorId> {
        &self.neighbors[layer]
    }

    pub fn neighbors_mut(&mut self, layer: usize) -> &mut HashSet<VectorId> {
        &mut self.neighbors[layer]
    }

    pub fn set_level(&mut self, level: usize) {
        self.level = level;
        self.neighbors.resize(level + 1, HashSet::new());
    }

    pub fn add_neighbor(&mut self, layer: usize, neighbor: VectorId) {
        if layer >= self.neighbors.len() {
            self.neighbors.resize(layer + 1, HashSet::new());
        }
        self.neighbors[layer].insert(neighbor);
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, String> {
        serde_cbor::to_vec(self).map_err(|e| e.to_string())
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, String> {
        serde_cbor::from_slice(data).map_err(|e| e.to_string())
    }

    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }

    pub fn mark_deleted(&mut self) {
        self.is_deleted = true;
    }
}

#[derive(Clone, PartialEq)]
struct SearchCandidate {
    id: VectorId,
    distance: f32,
}

impl Eq for SearchCandidate {}

impl PartialOrd for SearchCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse order for min-heap
        other.distance.partial_cmp(&self.distance)
    }
}

impl Ord for SearchCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

pub struct HNSWIndex {
    config: HNSWConfig,
    nodes: Arc<RwLock<HashMap<VectorId, HNSWNode>>>,
    entry_point: Arc<RwLock<Option<VectorId>>>,
    rng: Arc<RwLock<StdRng>>,
    dimension: Arc<RwLock<Option<usize>>>,
    /// Chunk loader for lazy loading vectors from S5 storage
    chunk_loader: Option<Arc<ChunkLoader>>,
    /// Cache for lazy-loaded vectors (vector_id -> vector)
    vector_cache: Arc<RwLock<HashMap<VectorId, Vec<f32>>>>,
    /// Chunk references for lazy loading (vector_id -> chunk_path)
    chunk_refs: Arc<RwLock<HashMap<VectorId, String>>>,
}

impl HNSWIndex {
    pub fn new(config: HNSWConfig) -> Self {
        let rng = match config.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_entropy(),
        };

        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            entry_point: Arc::new(RwLock::new(None)),
            rng: Arc::new(RwLock::new(rng)),
            dimension: Arc::new(RwLock::new(None)),
            chunk_loader: None,
            vector_cache: Arc::new(RwLock::new(HashMap::new())),
            chunk_refs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new HNSW index with chunk loader for lazy loading support
    pub fn with_chunk_loader(config: HNSWConfig, chunk_loader: Option<Arc<ChunkLoader>>) -> Self {
        let rng = match config.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_entropy(),
        };

        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            entry_point: Arc::new(RwLock::new(None)),
            rng: Arc::new(RwLock::new(rng)),
            dimension: Arc::new(RwLock::new(None)),
            chunk_loader,
            vector_cache: Arc::new(RwLock::new(HashMap::new())),
            chunk_refs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn config(&self) -> &HNSWConfig {
        &self.config
    }

    pub fn node_count(&self) -> usize {
        self.nodes.read().unwrap().len()
    }

    pub fn entry_point(&self) -> Option<VectorId> {
        self.entry_point.read().unwrap().clone()
    }

    pub fn get_node(&self, id: &VectorId) -> Option<HNSWNode> {
        self.nodes.read().unwrap().get(id).cloned()
    }

    pub fn assign_level(&self) -> usize {
        let mut rng = self.rng.write().unwrap();

        // HNSW level assignment with adjusted probability
        // Use p = 0.408 to get ~59.2% at level 0 and ratios around 2.0-2.5
        let p = 0.408;

        let mut level = 0;
        while rng.gen::<f64>() < p {
            level += 1;
        }

        level
    }

    pub fn insert(&mut self, id: VectorId, vector: Vec<f32>) -> Result<(), HNSWError> {
        // Check if vector already exists
        if self.nodes.read().unwrap().contains_key(&id) {
            return Err(HNSWError::DuplicateVector(id));
        }

        // Check/set dimension
        {
            let mut dim_guard = self.dimension.write().unwrap();
            match *dim_guard {
                Some(dim) if dim != vector.len() => {
                    return Err(HNSWError::DimensionMismatch {
                        expected: dim,
                        actual: vector.len(),
                    });
                }
                None => *dim_guard = Some(vector.len()),
                _ => {}
            }
        }

        let level = self.assign_level();
        let mut node = HNSWNode::new(id.clone(), vector);
        node.set_level(level);

        // If this is the first node, set it as entry point
        let is_first = {
            let mut ep_guard = self.entry_point.write().unwrap();
            if ep_guard.is_none() {
                *ep_guard = Some(id.clone());
                true
            } else {
                false
            }
        };

        let entry_level = if !is_first {
            // Find nearest neighbors at all layers
            let entry_point = self.entry_point().unwrap();
            let ef = self.config.ef_construction;

            // Search for nearest neighbors starting from top layer of entry point
            let entry_node = self
                .nodes
                .read()
                .unwrap()
                .get(&entry_point)
                .unwrap()
                .clone();
            let entry_level = entry_node.level();

            let mut current_nearest = vec![SearchCandidate {
                id: entry_point.clone(),
                distance: euclidean_distance(&node.vector, &entry_node.vector),
            }];

            // Search from the minimum of the new node's level and entry point's level
            let search_level = level.min(entry_level);
            for lc in (0..=search_level).rev() {
                let candidates =
                    self.search_layer(&node.vector, current_nearest[0].id.clone(), 1, lc);
                if !candidates.is_empty() {
                    current_nearest = candidates;
                }
            }

            // Connect to neighbors at each layer
            for lc in 0..=level {
                let m = if lc == 0 {
                    self.config.max_connections_layer_0
                } else {
                    self.config.max_connections
                };

                let candidates = self.search_layer(&node.vector, entry_point.clone(), ef, lc);
                let neighbors = self.select_neighbors(&candidates, m);

                // Add bidirectional connections
                {
                    let mut nodes_guard = self.nodes.write().unwrap();

                    // Add neighbors to new node
                    for neighbor_id in &neighbors {
                        node.neighbors_mut(lc).insert(neighbor_id.clone());
                    }

                    // Add new node to neighbors and collect pruning info
                    let max_conn = if lc == 0 {
                        self.config.max_connections_layer_0
                    } else {
                        self.config.max_connections
                    };

                    let mut pruning_needed = Vec::new();
                    for neighbor_id in &neighbors {
                        if let Some(neighbor) = nodes_guard.get_mut(neighbor_id) {
                            if neighbor.level >= lc {
                                neighbor.neighbors_mut(lc).insert(id.clone());

                                // Check if pruning needed
                                if neighbor.neighbors(lc).len() > max_conn {
                                    let neighbor_neighbors: Vec<_> =
                                        neighbor.neighbors(lc).iter().cloned().collect();
                                    let neighbor_vector = neighbor.vector().to_vec();
                                    pruning_needed.push((neighbor_id.clone(), neighbor_neighbors, neighbor_vector));
                                }
                            }
                        }
                    }

                    // Perform pruning (no mutable borrows held during prune_neighbors call)
                    //  Include new node vector for distance calculations
                    for (neighbor_id, neighbor_neighbors, neighbor_vector) in pruning_needed {
                        let pruned = self.prune_neighbors_with_new_node(
                            &neighbor_neighbors,
                            &neighbor_vector,
                            max_conn,
                            &nodes_guard,  // Pass nodes reference to avoid deadlock
                            &id,            // New node ID
                            &node.vector,   // New node vector
                        );
                        if let Some(neighbor) = nodes_guard.get_mut(&neighbor_id) {
                            neighbor.neighbors_mut(lc).clear();
                            for n in pruned {
                                neighbor.neighbors_mut(lc).insert(n);
                            }
                        }
                    }
                }
            }

            entry_level
        } else {
            0
        };

        // Insert node
        self.nodes.write().unwrap().insert(id.clone(), node);

        // Update entry point if new node has higher level
        if !is_first && level > entry_level {
            *self.entry_point.write().unwrap() = Some(id);
        }

        Ok(())
    }

    /// Insert a vector with chunk reference for lazy loading support
    pub fn insert_with_chunk(
        &mut self,
        id: VectorId,
        vector: Vec<f32>,
        chunk_id: Option<String>,
    ) -> Result<(), HNSWError> {
        // Store chunk reference if provided
        if let Some(chunk) = chunk_id {
            self.chunk_refs.write().unwrap().insert(id.clone(), chunk);
            // Cache the vector for immediate use
            self.vector_cache.write().unwrap().insert(id.clone(), vector.clone());
        }

        // Regular insert with the vector (needed for graph building)
        self.insert(id, vector)
    }

    pub fn search(
        &self,
        query: &[f32],
        k: usize,
        ef: usize,
    ) -> Result<Vec<SearchResult>, HNSWError> {
        let entry_point = match self.entry_point() {
            Some(ep) => ep,
            None => return Ok(Vec::new()), // Empty index
        };

        // Check dimension
        if let Some(dim) = *self.dimension.read().unwrap() {
            if query.len() != dim {
                return Err(HNSWError::DimensionMismatch {
                    expected: dim,
                    actual: query.len(),
                });
            }
        }

        // Start from top layer of entry point
        let nodes = self.nodes.read().unwrap();
        let entry_node = match nodes.get(&entry_point) {
            Some(node) => node,
            None => {
                return Err(HNSWError::ChunkLoadError(format!(
                    "Entry point node not found in index. Entry point: {:?}, Total nodes in memory: {}. This may indicate that lazy loading is enabled but the entry point was not properly loaded.",
                    entry_point, nodes.len()
                )));
            }
        };
        let top_layer = entry_node.level();

        let mut nearest = vec![SearchCandidate {
            id: entry_point.clone(),
            distance: euclidean_distance(query, &entry_node.vector),
        }];

        // Search through layers from top to layer 0
        for lc in (0..=top_layer).rev() {
            let new_nearest = self.search_layer(
                query,
                nearest[0].id.clone(),
                if lc == 0 { ef } else { 1 },
                lc,
            );
            if !new_nearest.is_empty() {
                nearest = new_nearest;
            }
        }

        // Return top k results
        nearest.truncate(k);
        Ok(nearest
            .into_iter()
            .map(|c| SearchResult::new(c.id, c.distance, None))
            .collect())
    }

    fn search_layer(
        &self,
        query: &[f32],
        entry_point: VectorId,
        ef: usize,
        layer: usize,
    ) -> Vec<SearchCandidate> {
        let nodes = self.nodes.read().unwrap();

        // Check if entry point exists
        if !nodes.contains_key(&entry_point) {
            return Vec::new();
        }

        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut nearest = BinaryHeap::new();

        let entry_distance = euclidean_distance(query, &nodes[&entry_point].vector);
        candidates.push(SearchCandidate {
            id: entry_point.clone(),
            distance: entry_distance,
        });
        nearest.push(SearchCandidate {
            id: entry_point.clone(),
            distance: -entry_distance, // Negative for max-heap
        });
        visited.insert(entry_point);

        while let Some(current) = candidates.pop() {
            if current.distance > -nearest.peek().unwrap().distance {
                break;
            }

            if let Some(node) = nodes.get(&current.id) {
                if node.level() >= layer {
                    for neighbor_id in node.neighbors(layer) {
                        if !visited.contains(neighbor_id) {
                            visited.insert(neighbor_id.clone());

                            if let Some(neighbor) = nodes.get(neighbor_id) {
                                // Skip deleted nodes
                                if neighbor.is_deleted() {
                                    continue;
                                }

                                let distance = euclidean_distance(query, &neighbor.vector);

                                if distance < -nearest.peek().unwrap().distance
                                    || nearest.len() < ef
                                {
                                    candidates.push(SearchCandidate {
                                        id: neighbor_id.clone(),
                                        distance,
                                    });
                                    nearest.push(SearchCandidate {
                                        id: neighbor_id.clone(),
                                        distance: -distance,
                                    });

                                    if nearest.len() > ef {
                                        nearest.pop();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert back to min-heap order
        let mut result: Vec<_> = nearest
            .into_iter()
            .map(|c| SearchCandidate {
                id: c.id,
                distance: -c.distance,
            })
            .collect();
        result.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        result
    }

    fn select_neighbors(&self, candidates: &[SearchCandidate], m: usize) -> Vec<VectorId> {
        candidates.iter().take(m).map(|c| c.id.clone()).collect()
    }

    fn prune_neighbors(
        &self,
        neighbors: &[VectorId],
        base_vector: &[f32],
        m: usize,
        nodes: &HashMap<VectorId, HNSWNode>,  // Accept nodes reference to avoid deadlock
    ) -> Vec<VectorId> {
        // No lock acquisition here - use passed-in nodes reference
        let mut candidates: Vec<_> = neighbors
            .iter()
            .filter_map(|id| {
                nodes.get(id).map(|node| SearchCandidate {
                    id: id.clone(),
                    distance: euclidean_distance(base_vector, &node.vector),
                })
            })
            .collect();

        candidates.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        candidates.truncate(m);
        candidates.into_iter().map(|c| c.id).collect()
    }

    /// Prune neighbors while considering a new node that's not yet in the nodes map
    fn prune_neighbors_with_new_node(
        &self,
        neighbors: &[VectorId],
        base_vector: &[f32],
        m: usize,
        nodes: &HashMap<VectorId, HNSWNode>,
        new_node_id: &VectorId,
        new_node_vector: &[f32],
    ) -> Vec<VectorId> {
        // Include both existing nodes and the new node in distance calculations
        let mut candidates: Vec<_> = neighbors
            .iter()
            .filter_map(|id| {
                if id == new_node_id {
                    // Use the provided new node vector
                    Some(SearchCandidate {
                        id: id.clone(),
                        distance: euclidean_distance(base_vector, new_node_vector),
                    })
                } else {
                    // Look up existing nodes
                    nodes.get(id).map(|node| SearchCandidate {
                        id: id.clone(),
                        distance: euclidean_distance(base_vector, &node.vector),
                    })
                }
            })
            .collect();

        candidates.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        candidates.truncate(m);
        candidates.into_iter().map(|c| c.id).collect()
    }

    pub fn get_all_nodes(&self) -> Vec<HNSWNode> {
        self.nodes.read().unwrap().values().cloned().collect()
    }

    pub fn restore_node(&mut self, node: HNSWNode) -> Result<(), HNSWError> {
        // Set dimension if not set
        if let Ok(mut dim_guard) = self.dimension.write() {
            if dim_guard.is_none() {
                *dim_guard = Some(node.vector().len());
            }
        }

        let id = node.id().clone();
        self.nodes.write().unwrap().insert(id, node);
        Ok(())
    }

    pub fn set_entry_point(&mut self, id: VectorId) {
        *self.entry_point.write().unwrap() = Some(id);
    }

    pub fn get_node_index(&self, id: &VectorId) -> Option<usize> {
        // For now, return a simple hash-based index
        // In production, this would map to actual storage indices
        self.nodes.read().unwrap().get(id).map(|_| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            Hash::hash(id, &mut hasher);
            Hasher::finish(&hasher) as usize
        })
    }

    pub fn dimension(&self) -> Option<usize> {
        *self.dimension.read().unwrap()
    }

    pub fn nodes(&self) -> &Arc<RwLock<HashMap<VectorId, HNSWNode>>> {
        &self.nodes
    }

    /// Get the maximum level across all nodes (number of layers - 1)
    pub fn get_max_level(&self) -> usize {
        let nodes = self.nodes.read().unwrap();
        nodes.values().map(|node| node.level()).max().unwrap_or(0)
    }

    /// Get the number of nodes at each level
    pub fn get_level_distribution(&self) -> Vec<usize> {
        let nodes = self.nodes.read().unwrap();
        let max_level = nodes.values().map(|node| node.level()).max().unwrap_or(0);

        let mut distribution = vec![0; max_level + 1];
        for node in nodes.values() {
            for level in 0..=node.level() {
                distribution[level] += 1;
            }
        }
        distribution
    }
}

// Thread-safe wrapper implementation
unsafe impl Send for HNSWIndex {}
unsafe impl Sync for HNSWIndex {}

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
