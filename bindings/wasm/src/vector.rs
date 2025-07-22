use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[wasm_bindgen]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vector {
    data: Vec<f32>,
}

#[wasm_bindgen]
impl Vector {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<f32>) -> Self {
        Vector { data }
    }

    #[wasm_bindgen]
    pub fn dimension(&self) -> usize {
        self.data.len()
    }

    #[wasm_bindgen]
    pub fn get(&self, index: usize) -> Result<f32, JsValue> {
        self.data.get(index)
            .copied()
            .ok_or_else(|| JsValue::from_str(&format!("Index {} out of bounds", index)))
    }

    #[wasm_bindgen]
    pub fn normalize(&self) -> Vector {
        let magnitude = self.magnitude();
        if magnitude > 0.0 {
            let normalized: Vec<f32> = self.data.iter()
                .map(|&x| x / magnitude)
                .collect();
            Vector::new(normalized)
        } else {
            self.clone()
        }
    }

    #[wasm_bindgen]
    pub fn magnitude(&self) -> f32 {
        self.data.iter()
            .map(|&x| x * x)
            .sum::<f32>()
            .sqrt()
    }

    // Internal method, not exposed to WASM
    pub(crate) fn as_slice(&self) -> &[f32] {
        &self.data
    }
}

#[wasm_bindgen]
pub struct VectorBatch {
    vectors: Vec<Vector>,
}

#[wasm_bindgen]
impl VectorBatch {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        VectorBatch { vectors: Vec::new() }
    }

    #[wasm_bindgen]
    pub fn add_vector(&mut self, vector: Vector) {
        self.vectors.push(vector);
    }

    #[wasm_bindgen]
    pub fn length(&self) -> usize {
        self.vectors.len()
    }

    #[wasm_bindgen]
    pub fn compute_similarities(&self, query: &Vector) -> Vec<f32> {
        self.vectors.iter()
            .map(|v| cosine_similarity_internal(v.as_slice(), query.as_slice()))
            .collect()
    }
}

#[wasm_bindgen]
pub fn cosine_similarity(vec1: &Vector, vec2: &Vector) -> Result<f32, JsValue> {
    if vec1.dimension() != vec2.dimension() {
        return Err(JsValue::from_str(&format!(
            "Dimension mismatch: {} != {}",
            vec1.dimension(),
            vec2.dimension()
        )));
    }
    
    Ok(cosine_similarity_internal(vec1.as_slice(), vec2.as_slice()))
}

#[wasm_bindgen]
pub fn euclidean_distance(vec1: &Vector, vec2: &Vector) -> Result<f32, JsValue> {
    if vec1.dimension() != vec2.dimension() {
        return Err(JsValue::from_str(&format!(
            "Dimension mismatch: {} != {}",
            vec1.dimension(),
            vec2.dimension()
        )));
    }
    
    let sum_squared: f32 = vec1.data.iter()
        .zip(vec2.data.iter())
        .map(|(&a, &b)| {
            let diff = a - b;
            diff * diff
        })
        .sum();
    
    Ok(sum_squared.sqrt())
}

// Internal helper function
pub(crate) fn cosine_similarity_internal(vec1: &[f32], vec2: &[f32]) -> f32 {
    let dot_product: f32 = vec1.iter()
        .zip(vec2.iter())
        .map(|(&a, &b)| a * b)
        .sum();
    
    let magnitude1 = vec1.iter().map(|&x| x * x).sum::<f32>().sqrt();
    let magnitude2 = vec2.iter().map(|&x| x * x).sum::<f32>().sqrt();
    
    if magnitude1 > 0.0 && magnitude2 > 0.0 {
        dot_product / (magnitude1 * magnitude2)
    } else {
        0.0
    }
}

// SIMD version (uses browser's SIMD if available)
#[wasm_bindgen]
pub fn cosine_similarity_simd(vec1: &Vector, vec2: &Vector) -> Result<f32, JsValue> {
    // In a real implementation, this would use SIMD instructions
    // For now, we'll use the regular implementation
    // WASM SIMD proposal is still evolving
    cosine_similarity(vec1, vec2)
}