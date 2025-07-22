use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{to_value, from_value};
use std::collections::HashMap;
use crate::vector::cosine_similarity_internal;

#[wasm_bindgen]
pub struct SearchResult {
    id: String,
    distance: f32,
    metadata: JsValue,
}

#[wasm_bindgen]
impl SearchResult {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn distance(&self) -> f32 {
        self.distance
    }

    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> JsValue {
        self.metadata.clone()
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct VectorEntry {
    id: String,
    vector: Vec<f32>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
pub struct InMemoryIndex {
    dimension: usize,
    vectors: Vec<VectorEntry>,
}

#[wasm_bindgen]
impl InMemoryIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(dimension: usize) -> Self {
        InMemoryIndex {
            dimension,
            vectors: Vec::new(),
        }
    }

    #[wasm_bindgen]
    pub fn add_vector(&mut self, id: &str, vector: Vec<f32>) -> Result<(), JsValue> {
        if id.is_empty() {
            return Err(JsValue::from_str("Vector ID cannot be empty"));
        }
        
        if vector.len() != self.dimension {
            return Err(JsValue::from_str(&format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            )));
        }
        
        // Check for duplicate ID
        if self.vectors.iter().any(|v| v.id == id) {
            return Err(JsValue::from_str(&format!("Vector with ID '{}' already exists", id)));
        }
        
        self.vectors.push(VectorEntry {
            id: id.to_string(),
            vector,
            metadata: None,
        });
        
        Ok(())
    }

    #[wasm_bindgen]
    pub fn add_vector_with_metadata(
        &mut self,
        id: &str,
        vector: Vec<f32>,
        metadata: JsValue
    ) -> Result<(), JsValue> {
        if id.is_empty() {
            return Err(JsValue::from_str("Vector ID cannot be empty"));
        }
        
        if vector.len() != self.dimension {
            return Err(JsValue::from_str(&format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            )));
        }
        
        // Check for duplicate ID
        if self.vectors.iter().any(|v| v.id == id) {
            return Err(JsValue::from_str(&format!("Vector with ID '{}' already exists", id)));
        }
        
        // Convert JsValue to HashMap
        let metadata_map: HashMap<String, serde_json::Value> = from_value(metadata)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse metadata: {}", e)))?;
        
        self.vectors.push(VectorEntry {
            id: id.to_string(),
            vector,
            metadata: Some(metadata_map),
        });
        
        Ok(())
    }

    #[wasm_bindgen]
    pub fn search(&self, query: Vec<f32>, k: usize) -> Result<Vec<SearchResult>, JsValue> {
        if query.len() != self.dimension {
            return Err(JsValue::from_str(&format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }
        
        let mut results: Vec<(usize, f32)> = self.vectors.iter()
            .enumerate()
            .map(|(idx, entry)| {
                let similarity = cosine_similarity_internal(&entry.vector, &query);
                let distance = 1.0 - similarity; // Convert similarity to distance
                (idx, distance)
            })
            .collect();
        
        // Sort by distance (ascending)
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Take top k results
        let top_k: Vec<SearchResult> = results.into_iter()
            .take(k)
            .map(|(idx, distance)| {
                let entry = &self.vectors[idx];
                let metadata = entry.metadata.as_ref()
                    .map(|m| to_value(m).unwrap_or(JsValue::NULL))
                    .unwrap_or(JsValue::NULL);
                
                SearchResult {
                    id: entry.id.clone(),
                    distance,
                    metadata,
                }
            })
            .collect();
        
        Ok(top_k)
    }

    #[wasm_bindgen]
    pub fn search_with_filter(
        &self,
        query: Vec<f32>,
        k: usize,
        filter: &SearchFilter
    ) -> Result<Vec<SearchResult>, JsValue> {
        if query.len() != self.dimension {
            return Err(JsValue::from_str(&format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }
        
        let mut results: Vec<(usize, f32)> = self.vectors.iter()
            .enumerate()
            .filter(|(_, entry)| filter.matches(entry))
            .map(|(idx, entry)| {
                let similarity = cosine_similarity_internal(&entry.vector, &query);
                let distance = 1.0 - similarity;
                (idx, distance)
            })
            .collect();
        
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        let top_k: Vec<SearchResult> = results.into_iter()
            .take(k)
            .map(|(idx, distance)| {
                let entry = &self.vectors[idx];
                let metadata = entry.metadata.as_ref()
                    .map(|m| to_value(m).unwrap_or(JsValue::NULL))
                    .unwrap_or(JsValue::NULL);
                
                SearchResult {
                    id: entry.id.clone(),
                    distance,
                    metadata,
                }
            })
            .collect();
        
        Ok(top_k)
    }

    #[wasm_bindgen]
    pub fn update_vector(&mut self, id: &str, vector: Vec<f32>) -> Result<(), JsValue> {
        if vector.len() != self.dimension {
            return Err(JsValue::from_str(&format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            )));
        }
        
        match self.vectors.iter_mut().find(|v| v.id == id) {
            Some(entry) => {
                entry.vector = vector;
                Ok(())
            }
            None => Err(JsValue::from_str(&format!("Vector with ID '{}' not found", id)))
        }
    }

    #[wasm_bindgen]
    pub fn delete_vector(&mut self, id: &str) -> Result<(), JsValue> {
        let initial_len = self.vectors.len();
        self.vectors.retain(|v| v.id != id);
        
        if self.vectors.len() < initial_len {
            Ok(())
        } else {
            Err(JsValue::from_str(&format!("Vector with ID '{}' not found", id)))
        }
    }

    #[wasm_bindgen]
    pub fn size(&self) -> usize {
        self.vectors.len()
    }

    #[wasm_bindgen]
    pub fn serialize(&self) -> Result<Vec<u8>, JsValue> {
        bincode::serialize(self)
            .map_err(|e| JsValue::from_str(&format!("Serialization failed: {}", e)))
    }

    #[wasm_bindgen]
    pub fn deserialize(data: &[u8]) -> Result<InMemoryIndex, JsValue> {
        bincode::deserialize(data)
            .map_err(|e| JsValue::from_str(&format!("Deserialization failed: {}", e)))
    }
}

#[wasm_bindgen]
pub struct SearchFilter {
    string_filters: HashMap<String, String>,
    number_filters: HashMap<String, (String, f64)>, // field -> (operator, value)
}

#[wasm_bindgen]
impl SearchFilter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        SearchFilter {
            string_filters: HashMap::new(),
            number_filters: HashMap::new(),
        }
    }

    #[wasm_bindgen]
    pub fn add_string_filter(&mut self, field: &str, value: &str) {
        self.string_filters.insert(field.to_string(), value.to_string());
    }

    #[wasm_bindgen]
    pub fn add_number_filter(&mut self, field: &str, operator: &str, value: f64) {
        self.number_filters.insert(field.to_string(), (operator.to_string(), value));
    }

    fn matches(&self, entry: &VectorEntry) -> bool {
        if let Some(metadata) = &entry.metadata {
            // Check string filters
            for (field, expected_value) in &self.string_filters {
                if let Some(actual_value) = metadata.get(field) {
                    if let Some(str_value) = actual_value.as_str() {
                        if str_value != expected_value {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            
            // Check number filters
            for (field, (operator, expected_value)) in &self.number_filters {
                if let Some(actual_value) = metadata.get(field) {
                    if let Some(num_value) = actual_value.as_f64() {
                        let matches = match operator.as_str() {
                            "eq" => num_value == *expected_value,
                            "ne" => num_value != *expected_value,
                            "gt" => num_value > *expected_value,
                            "gte" => num_value >= *expected_value,
                            "lt" => num_value < *expected_value,
                            "lte" => num_value <= *expected_value,
                            _ => false,
                        };
                        if !matches {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            
            true
        } else {
            // No metadata, so filters don't match
            self.string_filters.is_empty() && self.number_filters.is_empty()
        }
    }
}