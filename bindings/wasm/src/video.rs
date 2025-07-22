use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::vector::cosine_similarity_internal;

#[derive(Clone, Serialize, Deserialize)]
struct VideoEntry {
    id: String,
    embedding: Vec<f32>,
    tags: Vec<String>,
    metadata: HashMap<String, serde_json::Value>,
}

#[wasm_bindgen]
pub struct VideoSimilarityIndex {
    videos: Vec<VideoEntry>,
}

#[wasm_bindgen]
#[derive(Serialize)]
pub struct SimilarVideo {
    id: String,
    similarity: f32,
}

#[wasm_bindgen]
impl SimilarVideo {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn similarity(&self) -> f32 {
        self.similarity
    }
}

#[wasm_bindgen]
impl VideoSimilarityIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        VideoSimilarityIndex {
            videos: Vec::new(),
        }
    }

    #[wasm_bindgen]
    pub fn add_video(&mut self, id: &str, embedding: Vec<f32>, tags: Vec<String>) {
        self.videos.push(VideoEntry {
            id: id.to_string(),
            embedding,
            tags,
            metadata: HashMap::new(),
        });
    }

    #[wasm_bindgen]
    pub fn find_similar(&self, video_id: &str, k: usize) -> Result<Vec<SimilarVideo>, JsValue> {
        // Find the query video
        let query_video = self.videos.iter()
            .find(|v| v.id == video_id)
            .ok_or_else(|| JsValue::from_str(&format!("Video '{}' not found", video_id)))?;
        
        let query_embedding = &query_video.embedding;
        
        // Calculate similarities
        let mut similarities: Vec<(String, f32)> = self.videos.iter()
            .map(|video| {
                let similarity = cosine_similarity_internal(&video.embedding, query_embedding);
                (video.id.clone(), similarity)
            })
            .collect();
        
        // Sort by similarity (descending)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Take top k
        let results: Vec<SimilarVideo> = similarities.into_iter()
            .take(k)
            .map(|(id, similarity)| SimilarVideo { id, similarity })
            .collect();
        
        Ok(results)
    }
}

#[wasm_bindgen]
pub struct VideoRecommender {
    videos: HashMap<String, VideoEntry>,
}

#[wasm_bindgen]
#[derive(Serialize)]
pub struct VideoRecommendation {
    id: String,
    score: f32,
    category: String,
}

#[wasm_bindgen]
impl VideoRecommendation {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn score(&self) -> f32 {
        self.score
    }

    #[wasm_bindgen(getter)]
    pub fn category(&self) -> String {
        self.category.clone()
    }
}

#[wasm_bindgen]
impl VideoRecommender {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        VideoRecommender {
            videos: HashMap::new(),
        }
    }

    #[wasm_bindgen]
    pub fn add_video(&mut self, id: &str, embedding: Vec<f32>, category: &str) {
        let mut metadata = HashMap::new();
        metadata.insert("category".to_string(), serde_json::Value::String(category.to_string()));
        
        self.videos.insert(id.to_string(), VideoEntry {
            id: id.to_string(),
            embedding,
            tags: vec![],
            metadata,
        });
    }

    #[wasm_bindgen]
    pub fn recommend_from_history(&self, watch_history: Vec<String>, k: usize) -> Vec<VideoRecommendation> {
        // Calculate average embedding from watch history
        let mut avg_embedding: Vec<f32> = vec![];
        let mut count = 0;
        
        for video_id in &watch_history {
            if let Some(video) = self.videos.get(video_id) {
                if avg_embedding.is_empty() {
                    avg_embedding = video.embedding.clone();
                } else {
                    for (i, &val) in video.embedding.iter().enumerate() {
                        avg_embedding[i] += val;
                    }
                }
                count += 1;
            }
        }
        
        if count == 0 || avg_embedding.is_empty() {
            return vec![];
        }
        
        // Normalize average embedding
        for val in &mut avg_embedding {
            *val /= count as f32;
        }
        
        // Find similar videos
        let mut recommendations: Vec<(String, f32, String)> = self.videos.iter()
            .filter(|(id, _)| !watch_history.contains(id))
            .map(|(id, video)| {
                let similarity = cosine_similarity_internal(&video.embedding, &avg_embedding);
                let category = video.metadata.get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                (id.clone(), similarity, category)
            })
            .collect();
        
        // Sort by similarity
        recommendations.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Convert to recommendations
        recommendations.into_iter()
            .take(k)
            .map(|(id, score, category)| VideoRecommendation { id, score, category })
            .collect()
    }
}

#[wasm_bindgen]
pub struct VideoClustering {
    videos: Vec<VideoEntry>,
}

#[wasm_bindgen]
#[derive(Serialize)]
pub struct VideoCluster {
    cluster_id: usize,
    centroid: Vec<f32>,
    video_ids: Vec<String>,
}

#[wasm_bindgen]
impl VideoCluster {
    #[wasm_bindgen(getter)]
    pub fn cluster_id(&self) -> usize {
        self.cluster_id
    }

    #[wasm_bindgen(getter)]
    pub fn centroid(&self) -> Vec<f32> {
        self.centroid.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn video_ids(&self) -> Vec<String> {
        self.video_ids.clone()
    }
}

#[wasm_bindgen]
impl VideoClustering {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        VideoClustering {
            videos: Vec::new(),
        }
    }

    #[wasm_bindgen]
    pub fn add_video(&mut self, id: &str, embedding: Vec<f32>) {
        self.videos.push(VideoEntry {
            id: id.to_string(),
            embedding,
            tags: vec![],
            metadata: HashMap::new(),
        });
    }

    #[wasm_bindgen]
    pub fn cluster(&self, k: usize) -> Vec<VideoCluster> {
        if self.videos.is_empty() || k == 0 {
            return vec![];
        }

        let k = k.min(self.videos.len());
        
        // Simple k-means clustering
        // Initialize centroids randomly
        let mut centroids: Vec<Vec<f32>> = Vec::new();
        let step = self.videos.len() / k;
        for i in 0..k {
            centroids.push(self.videos[i * step].embedding.clone());
        }
        
        // Iterate until convergence (or max iterations)
        let max_iterations = 20;
        let mut assignments: Vec<usize> = vec![0; self.videos.len()];
        
        for _ in 0..max_iterations {
            // Assign videos to nearest centroid
            let mut changed = false;
            for (i, video) in self.videos.iter().enumerate() {
                let mut min_distance = f32::MAX;
                let mut best_cluster = 0;
                
                for (j, centroid) in centroids.iter().enumerate() {
                    let similarity = cosine_similarity_internal(&video.embedding, centroid);
                    let distance = 1.0 - similarity;
                    
                    if distance < min_distance {
                        min_distance = distance;
                        best_cluster = j;
                    }
                }
                
                if assignments[i] != best_cluster {
                    assignments[i] = best_cluster;
                    changed = true;
                }
            }
            
            if !changed {
                break;
            }
            
            // Update centroids
            for (cluster_id, centroid) in centroids.iter_mut().enumerate() {
                let cluster_videos: Vec<&VideoEntry> = self.videos.iter()
                    .enumerate()
                    .filter(|(i, _)| assignments[*i] == cluster_id)
                    .map(|(_, v)| v)
                    .collect();
                
                if !cluster_videos.is_empty() {
                    // Calculate mean of cluster embeddings
                    let dim = centroid.len();
                    for i in 0..dim {
                        centroid[i] = cluster_videos.iter()
                            .map(|v| v.embedding[i])
                            .sum::<f32>() / cluster_videos.len() as f32;
                    }
                }
            }
        }
        
        // Build final clusters
        let mut clusters = Vec::new();
        for (cluster_id, centroid) in centroids.into_iter().enumerate() {
            let video_ids: Vec<String> = self.videos.iter()
                .enumerate()
                .filter(|(i, _)| assignments[*i] == cluster_id)
                .map(|(_, v)| v.id.clone())
                .collect();
            
            if !video_ids.is_empty() {
                clusters.push(VideoCluster {
                    cluster_id,
                    centroid,
                    video_ids,
                });
            }
        }
        
        clusters
    }
}