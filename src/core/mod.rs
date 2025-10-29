// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

pub mod chunk;
pub mod chunk_cache;
pub mod storage;
pub mod types;
pub mod vector_ops;

pub use types::{Vector, VectorId, Embedding, VideoMetadata};
pub use chunk::{
    VectorChunk, ChunkMetadata, Manifest, HNSWManifest, IVFManifest,
    LayerMetadata, ChunkError, MANIFEST_VERSION,
};
pub use chunk_cache::{ChunkCache, CacheMetrics};
