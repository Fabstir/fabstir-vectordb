use wasm_bindgen::prelude::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

pub mod utils;
pub mod vector;
pub mod index;
pub mod video;

// Re-export main types
pub use vector::{Vector, VectorBatch, cosine_similarity, euclidean_distance, cosine_similarity_simd};
pub use index::{InMemoryIndex, SearchFilter, SearchResult};
pub use video::{VideoSimilarityIndex, VideoRecommender, VideoClustering, VideoCluster};
pub use utils::{get_system_info, SystemInfo};

// Initialize panic hook for better error messages in the browser
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}