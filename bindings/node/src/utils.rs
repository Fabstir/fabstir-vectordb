/// Helper to convert JS array to Rust vector
#[allow(dead_code)]
pub fn js_array_to_vec_f32(arr: Vec<f64>) -> Vec<f32> {
    arr.into_iter().map(|v| v as f32).collect()
}

/// Helper to convert Rust vector to JS array
#[allow(dead_code)]
pub fn vec_f32_to_js_array(vec: Vec<f32>) -> Vec<f64> {
    vec.into_iter().map(|v| v as f64).collect()
}
