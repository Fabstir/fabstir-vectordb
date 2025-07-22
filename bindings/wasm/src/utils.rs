use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
pub struct SystemInfo {
    #[wasm_bindgen(getter_with_clone)]
    pub simd_available: bool,
    #[wasm_bindgen(getter_with_clone)]
    pub threads_available: bool,
    #[wasm_bindgen(getter_with_clone)]
    pub memory_pages: u32,
}

#[wasm_bindgen]
pub fn get_system_info() -> SystemInfo {
    // Check for WASM features
    // In a real implementation, this would query browser capabilities
    let simd_available = false; // Would check for WASM SIMD support
    let threads_available = false; // Would check for WASM threads support
    
    // Get current memory usage - WebAssembly page size is 64KB
    let memory_pages = 16; // Default 1MB allocation (16 * 64KB)

    SystemInfo {
        simd_available,
        threads_available,
        memory_pages,
    }
}

// Helper function to log to browser console
#[wasm_bindgen]
pub fn log(msg: &str) {
    web_sys::console::log_1(&JsValue::from_str(msg));
}