# Vector DB Node.js Native Bindings - Implementation Plan

**Status:** Implementation Ready
**Priority:** High
**Estimated Time:** 2-3 days
**Target:** Node.js N-API bindings for fabstir-vectordb

---

## Overview

This document details the implementation plan for creating Node.js native bindings for fabstir-vectordb. The bindings will enable embedding the vector database directly into fabstir-llm-sdk as a native module, eliminating the need for a separate server and maintaining the P2P decentralized architecture.

### Goals

1. **Embed in SDK:** Package as standard npm module with prebuilt binaries
2. **Session-Based:** Support user-specific vector sessions with S5 storage
3. **High Performance:** Native Rust performance for vector operations
4. **Stateless:** Clean memory on session end (host remains stateless)
5. **Full S5 Integration:** Leverage existing enhanced S5 storage code

### Non-Goals

- ❌ HTTP server or REST API (use bindings/wasm for that)
- ❌ Multi-user shared state (sessions are isolated)
- ❌ Persistent host storage (everything from S5)

---

## Architecture

### Stack

```
fabstir-llm-sdk (TypeScript/JavaScript)
    ↓
@fabstir/vector-db-native (npm package)
    ↓
napi-rs (Rust ↔ Node.js bridge)
    ↓
fabstir-vectordb core (existing Rust code)
    ├── src/storage/enhanced_s5_storage.rs
    ├── src/hybrid/ (HNSW + IVF)
    └── src/core/ (vector operations)
```

### Data Flow

```
1. User starts session
   ↓
2. SDK creates VectorDBSession
   ↓
3. Native module loads vectors from S5
   ↓
4. Search operations in-memory (Rust)
   ↓
5. Results returned to JS
   ↓
6. Session ends → destroy() clears memory
```

---

## Directory Structure

```
fabstir-vectordb/
├── bindings/
│   └── node/
│       ├── Cargo.toml           # Rust dependencies
│       ├── build.rs             # Build script
│       ├── package.json         # npm configuration
│       ├── index.d.ts           # TypeScript definitions
│       ├── README.md            # Integration guide
│       ├── .npmignore           # npm packaging
│       └── src/
│           ├── lib.rs           # Main entry point
│           ├── session.rs       # VectorDBSession implementation
│           ├── error.rs         # Error types
│           ├── utils.rs         # Helper functions
│           └── types.rs         # Type conversions
├── src/                         # Existing Rust core (reuse)
│   ├── storage/
│   ├── hybrid/
│   ├── core/
│   └── ...
└── Cargo.toml                   # Workspace configuration
```

---

## Phase 1: Setup Infrastructure

### 1.1 Create Directory Structure

```bash
cd fabstir-vectordb
mkdir -p bindings/node/src
```

### 1.2 Configure Cargo Workspace

Update root `Cargo.toml`:

```toml
[workspace]
members = [
    ".",
    "bindings/wasm",
    "bindings/node"  # Add this
]
```

### 1.3 Create bindings/node/Cargo.toml

```toml
[package]
name = "vector-db-native"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Core vector-db (use workspace dependency)
vector-db = { path = "../..", default-features = false }

# napi-rs for Node.js bindings
napi = { version = "2.16", features = ["tokio_rt", "async", "napi8"] }
napi-derive = "2.16"

# Async runtime
tokio = { version = "1.35", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

[build-dependencies]
napi-build = "2.1"

[profile.release]
lto = true
codegen-units = 1
opt-level = 3
```

### 1.4 Create bindings/node/build.rs

```rust
fn main() {
    napi_build::setup();
}
```

### 1.5 Create bindings/node/package.json

```json
{
  "name": "@fabstir/vector-db-native",
  "version": "0.1.0",
  "description": "Native Node.js bindings for Fabstir Vector Database",
  "main": "index.js",
  "types": "index.d.ts",
  "napi": {
    "name": "vector-db-native",
    "triples": {
      "defaults": true,
      "additional": [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu"
      ]
    }
  },
  "scripts": {
    "build": "napi build --platform --release --pipe \"prettier -w\"",
    "build:debug": "napi build --platform",
    "prepublishOnly": "napi prepublish -t npm",
    "test": "node --test",
    "version": "napi version"
  },
  "keywords": [
    "vector-database",
    "embeddings",
    "native",
    "napi-rs",
    "s5",
    "decentralized"
  ],
  "author": "Fabstir",
  "license": "MIT",
  "engines": {
    "node": ">= 16"
  },
  "devDependencies": {
    "@napi-rs/cli": "^2.18.0",
    "prettier": "^3.2.0"
  },
  "files": [
    "index.js",
    "index.d.ts",
    "*.node"
  ]
}
```

### 1.6 Install napi-rs CLI

```bash
cd bindings/node
npm install
```

---

## Phase 2: Core Bindings Implementation

### 2.1 Create bindings/node/src/lib.rs

```rust
#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

mod error;
mod session;
mod types;
mod utils;

pub use error::{VectorDBError, Result};
pub use session::VectorDBSession;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[napi(object)]
pub struct PlatformInfo {
    pub platform: String,
    pub arch: String,
}
```

### 2.2 Create bindings/node/src/error.rs

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;

pub type Result<T> = std::result::Result<T, VectorDBError>;

#[napi]
#[derive(Debug, Clone)]
pub struct VectorDBError {
    pub message: String,
    pub code: String,
}

impl VectorDBError {
    pub fn new(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: code.into(),
        }
    }

    pub fn s5_error(message: impl Into<String>) -> Self {
        Self::new(message, "S5_ERROR")
    }

    pub fn storage_error(message: impl Into<String>) -> Self {
        Self::new(message, "STORAGE_ERROR")
    }

    pub fn index_error(message: impl Into<String>) -> Self {
        Self::new(message, "INDEX_ERROR")
    }

    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::new(message, "INVALID_CONFIG")
    }

    pub fn session_error(message: impl Into<String>) -> Self {
        Self::new(message, "SESSION_ERROR")
    }
}

impl std::fmt::Display for VectorDBError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for VectorDBError {}

impl From<VectorDBError> for Error {
    fn from(err: VectorDBError) -> Self {
        Error::new(Status::GenericFailure, err.message)
    }
}

// Convert from core vector-db errors
impl From<vector_db::core::VectorError> for VectorDBError {
    fn from(err: vector_db::core::VectorError) -> Self {
        VectorDBError::index_error(err.to_string())
    }
}

impl From<anyhow::Error> for VectorDBError {
    fn from(err: anyhow::Error) -> Self {
        VectorDBError::new(err.to_string(), "INTERNAL_ERROR")
    }
}
```

### 2.3 Create bindings/node/src/types.rs

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct VectorDBConfig {
    /// S5 portal URL (e.g., "http://localhost:5524")
    pub s5_portal: String,

    /// User's blockchain-derived seed phrase
    pub user_seed_phrase: String,

    /// Unique session identifier
    pub session_id: String,

    /// Optional: Memory budget in MB (default: 512)
    pub memory_budget_mb: Option<u32>,

    /// Optional: Enable debug logging (default: false)
    pub debug: Option<bool>,
}

#[napi(object)]
pub struct LoadOptions {
    /// Load HNSW immediately, IVF on-demand (default: true)
    pub lazy_load: Option<bool>,

    /// Override session memory budget
    pub memory_budget_mb: Option<u32>,
}

#[napi(object)]
pub struct SearchOptions {
    /// Minimum similarity score (0-1, default: 0.7)
    pub threshold: Option<f32>,

    /// Include vectors in results (default: false)
    pub include_vectors: Option<bool>,
}

#[napi(object)]
pub struct VectorInput {
    /// Unique identifier
    pub id: String,

    /// Dense embedding vector
    pub vector: Vec<f32>,

    /// Associated metadata (JSON-serializable)
    pub metadata: serde_json::Value,
}

#[napi(object)]
pub struct SearchResult {
    /// Vector ID
    pub id: String,

    /// Similarity score (0-1)
    pub score: f32,

    /// Associated metadata
    pub metadata: serde_json::Value,

    /// Original vector (if requested)
    pub vector: Option<Vec<f32>>,
}

#[napi(object)]
pub struct SessionStats {
    /// Total vectors in index
    pub vector_count: u32,

    /// Current memory usage in MB
    pub memory_usage_mb: f32,

    /// Active index type
    pub index_type: String,

    /// Vectors in HNSW index
    pub hnsw_vector_count: Option<u32>,

    /// Vectors in IVF index
    pub ivf_vector_count: Option<u32>,
}
```

### 2.4 Create bindings/node/src/session.rs

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;
use tokio::sync::RwLock;

use vector_db::{
    hybrid::HybridIndex,
    storage::{EnhancedS5Storage, S5StorageConfig},
    core::Vector,
};

use crate::{
    error::{Result, VectorDBError},
    types::*,
};

struct SessionState {
    session_id: String,
    index: Arc<RwLock<HybridIndex>>,
    storage: Arc<EnhancedS5Storage>,
    config: VectorDBConfig,
}

#[napi]
pub struct VectorDBSession {
    state: Option<SessionState>,
}

#[napi]
impl VectorDBSession {
    /// Create a new vector DB session
    #[napi(factory)]
    pub async fn create(config: VectorDBConfig) -> Result<Self> {
        // Validate config
        if config.s5_portal.is_empty() {
            return Err(VectorDBError::invalid_config("s5_portal is required"));
        }
        if config.user_seed_phrase.is_empty() {
            return Err(VectorDBError::invalid_config("user_seed_phrase is required"));
        }
        if config.session_id.is_empty() {
            return Err(VectorDBError::invalid_config("session_id is required"));
        }

        // Create S5 storage
        let storage_config = S5StorageConfig {
            portal_url: config.s5_portal.clone(),
            seed_phrase: Some(config.user_seed_phrase.clone()),
            timeout_ms: Some(30000), // 30s timeout for S5 operations
            max_retries: Some(3),
        };

        let storage = EnhancedS5Storage::new(storage_config)
            .await
            .map_err(|e| VectorDBError::s5_error(format!("Failed to initialize S5: {}", e)))?;

        // Create hybrid index
        let index = HybridIndex::new(Default::default())
            .map_err(|e| VectorDBError::index_error(format!("Failed to create index: {}", e)))?;

        let state = SessionState {
            session_id: config.session_id.clone(),
            index: Arc::new(RwLock::new(index)),
            storage: Arc::new(storage),
            config,
        };

        Ok(Self { state: Some(state) })
    }

    /// Load user's vectors from S5
    #[napi]
    pub async fn load_user_vectors(
        &mut self,
        cid: String,
        options: Option<LoadOptions>,
    ) -> Result<()> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        let lazy_load = options.as_ref()
            .and_then(|o| o.lazy_load)
            .unwrap_or(true);

        // Load index from S5
        let index_data = state.storage
            .load(&cid)
            .await
            .map_err(|e| VectorDBError::storage_error(format!("Failed to load from S5: {}", e)))?;

        // Deserialize and load into index
        let mut index = state.index.write().await;
        index.load_from_bytes(&index_data, lazy_load)
            .map_err(|e| VectorDBError::index_error(format!("Failed to load index: {}", e)))?;

        Ok(())
    }

    /// Search for similar vectors
    #[napi]
    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        k: u32,
        options: Option<SearchOptions>,
    ) -> Result<Vec<SearchResult>> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        let threshold = options.as_ref()
            .and_then(|o| o.threshold)
            .unwrap_or(0.7);

        let include_vectors = options.as_ref()
            .and_then(|o| o.include_vectors)
            .unwrap_or(false);

        // Perform search
        let index = state.index.read().await;
        let results = index.search(&query_vector, k as usize)
            .map_err(|e| VectorDBError::index_error(format!("Search failed: {}", e)))?;

        // Filter by threshold and convert to SearchResult
        let search_results = results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .map(|r| SearchResult {
                id: r.id,
                score: r.score,
                metadata: r.metadata,
                vector: if include_vectors { Some(r.vector) } else { None },
            })
            .collect();

        Ok(search_results)
    }

    /// Add vectors to the index
    #[napi]
    pub async fn add_vectors(&mut self, vectors: Vec<VectorInput>) -> Result<()> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        let mut index = state.index.write().await;

        for input in vectors {
            let vector = Vector {
                id: input.id,
                data: input.vector,
                metadata: input.metadata,
            };

            index.add(vector)
                .map_err(|e| VectorDBError::index_error(format!("Failed to add vector: {}", e)))?;
        }

        Ok(())
    }

    /// Save index to S5
    #[napi]
    pub async fn save_to_s5(&self) -> Result<String> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        // Serialize index
        let index = state.index.read().await;
        let index_bytes = index.to_bytes()
            .map_err(|e| VectorDBError::index_error(format!("Failed to serialize index: {}", e)))?;

        // Upload to S5
        let cid = state.storage
            .store(&index_bytes)
            .await
            .map_err(|e| VectorDBError::storage_error(format!("Failed to save to S5: {}", e)))?;

        Ok(cid)
    }

    /// Get session statistics
    #[napi]
    pub fn get_stats(&self) -> Result<SessionStats> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        // This should be async in reality, but simplified for now
        let index = state.index.try_read()
            .map_err(|_| VectorDBError::session_error("Failed to read index stats"))?;

        let stats = index.stats();

        Ok(SessionStats {
            vector_count: stats.total_vectors as u32,
            memory_usage_mb: stats.memory_usage_bytes as f32 / 1_048_576.0,
            index_type: stats.index_type.to_string(),
            hnsw_vector_count: stats.hnsw_vectors.map(|v| v as u32),
            ivf_vector_count: stats.ivf_vectors.map(|v| v as u32),
        })
    }

    /// Destroy session and clear memory
    #[napi]
    pub async fn destroy(&mut self) -> Result<()> {
        if let Some(state) = self.state.take() {
            // Clear index
            let mut index = state.index.write().await;
            index.clear()
                .map_err(|e| VectorDBError::index_error(format!("Failed to clear index: {}", e)))?;

            drop(index);
            drop(state);
        }

        Ok(())
    }
}

// Ensure cleanup on drop
impl Drop for VectorDBSession {
    fn drop(&mut self) {
        if self.state.is_some() {
            eprintln!("WARNING: VectorDBSession dropped without calling destroy()");
        }
    }
}
```

### 2.5 Create bindings/node/src/utils.rs

```rust
use napi::bindgen_prelude::*;

/// Helper to convert JS array to Rust vector
pub fn js_array_to_vec_f32(arr: Vec<f64>) -> Vec<f32> {
    arr.into_iter().map(|v| v as f32).collect()
}

/// Helper to convert Rust vector to JS array
pub fn vec_f32_to_js_array(vec: Vec<f32>) -> Vec<f64> {
    vec.into_iter().map(|v| v as f64).collect()
}
```

---

## Phase 3: TypeScript Definitions

### 3.1 Create bindings/node/index.d.ts

```typescript
export class VectorDBSession {
  static create(config: VectorDBConfig): Promise<VectorDBSession>
  loadUserVectors(cid: string, options?: LoadOptions): Promise<void>
  search(
    queryVector: number[],
    k: number,
    options?: SearchOptions
  ): Promise<SearchResult[]>
  addVectors(vectors: VectorInput[]): Promise<void>
  saveToS5(): Promise<string>
  getStats(): SessionStats
  destroy(): Promise<void>
}

export interface VectorDBConfig {
  s5Portal: string
  userSeedPhrase: string
  sessionId: string
  memoryBudgetMB?: number
  debug?: boolean
}

export interface LoadOptions {
  lazyLoad?: boolean
  memoryBudgetMB?: number
}

export interface SearchOptions {
  threshold?: number
  includeVectors?: boolean
}

export interface VectorInput {
  id: string
  vector: number[]
  metadata: any
}

export interface SearchResult {
  id: string
  score: number
  metadata: any
  vector?: number[]
}

export interface SessionStats {
  vectorCount: number
  memoryUsageMB: number
  indexType: string
  hnswVectorCount?: number
  ivfVectorCount?: number
}

export class VectorDBError extends Error {
  code: string
  message: string
}

export function getVersion(): string
export function getPlatformInfo(): { platform: string; arch: string }
```

---

## Phase 4: Build System

### 4.1 Build Commands

```bash
# Development build
cd bindings/node
npm run build:debug

# Production build with optimizations
npm run build

# Build for specific platform
npm run build -- --target x86_64-unknown-linux-gnu
```

### 4.2 Create .npmignore

```
src/
Cargo.toml
Cargo.lock
build.rs
target/
*.node.debug
```

---

## Phase 5: Testing

### 5.1 Unit Tests

Create `bindings/node/test/session.test.js`:

```javascript
const { test } = require('node:test');
const assert = require('node:assert');
const { VectorDBSession } = require('..');

test('VectorDBSession.create', async (t) => {
  const session = await VectorDBSession.create({
    s5Portal: 'http://localhost:5524',
    userSeedPhrase: 'test seed phrase',
    sessionId: 'test-session-1',
    memoryBudgetMB: 256,
  });

  assert.ok(session, 'Session should be created');

  const stats = session.getStats();
  assert.strictEqual(stats.vectorCount, 0, 'Initial vector count should be 0');

  await session.destroy();
});

test('VectorDBSession.addVectors and search', async (t) => {
  const session = await VectorDBSession.create({
    s5Portal: 'http://localhost:5524',
    userSeedPhrase: 'test seed phrase',
    sessionId: 'test-session-2',
  });

  // Add test vectors
  await session.addVectors([
    {
      id: 'vec1',
      vector: new Array(384).fill(0.1),
      metadata: { text: 'Document 1' },
    },
    {
      id: 'vec2',
      vector: new Array(384).fill(0.2),
      metadata: { text: 'Document 2' },
    },
  ]);

  const stats = session.getStats();
  assert.strictEqual(stats.vectorCount, 2, 'Should have 2 vectors');

  // Search
  const queryVector = new Array(384).fill(0.15);
  const results = await session.search(queryVector, 2);

  assert.ok(results.length > 0, 'Should return results');
  assert.ok(results[0].score >= 0 && results[0].score <= 1, 'Score should be normalized');

  await session.destroy();
});

test('VectorDBSession.destroy cleans up', async (t) => {
  const session = await VectorDBSession.create({
    s5Portal: 'http://localhost:5524',
    userSeedPhrase: 'test seed phrase',
    sessionId: 'test-session-3',
  });

  await session.destroy();

  // Should throw after destroy
  await assert.rejects(
    async () => await session.search(new Array(384).fill(0), 5),
    /Session already destroyed/
  );
});
```

### 5.2 Integration Tests

Create `bindings/node/test/s5-integration.test.js`:

```javascript
const { test } = require('node:test');
const assert = require('node:assert');
const { VectorDBSession } = require('..');

// Skip if S5 not available
const S5_AVAILABLE = process.env.TEST_WITH_S5 === 'true';

test('S5 load and save', { skip: !S5_AVAILABLE }, async (t) => {
  const session = await VectorDBSession.create({
    s5Portal: process.env.S5_PORTAL_URL || 'http://localhost:5524',
    userSeedPhrase: process.env.TEST_SEED_PHRASE || 'test seed',
    sessionId: 'test-s5-1',
  });

  // Add vectors
  await session.addVectors([
    {
      id: 'test1',
      vector: new Array(384).fill(0.5),
      metadata: { source: 's5-test' },
    },
  ]);

  // Save to S5
  const cid = await session.saveToS5();
  assert.ok(cid, 'Should return CID');
  assert.ok(cid.startsWith('u'), 'CID should start with "u"');

  await session.destroy();

  // Load in new session
  const session2 = await VectorDBSession.create({
    s5Portal: process.env.S5_PORTAL_URL || 'http://localhost:5524',
    userSeedPhrase: process.env.TEST_SEED_PHRASE || 'test seed',
    sessionId: 'test-s5-2',
  });

  await session2.loadUserVectors(cid);

  const stats = session2.getStats();
  assert.strictEqual(stats.vectorCount, 1, 'Should load 1 vector from S5');

  await session2.destroy();
});
```

---

## Phase 6: Distribution

### 6.1 Create Tarball

```bash
cd bindings/node

# Build for production
npm run build

# Create tarball
npm pack

# Output: fabstir-vector-db-native-0.1.0.tgz
```

### 6.2 Installation by SDK

```bash
# From local tarball
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz

# From git repository (recommended for development)
npm install git+https://github.com/yourorg/fabstir-vectordb.git#main:bindings/node
```

---

## Phase 7: Documentation

### 7.1 Create bindings/node/README.md

```markdown
# @fabstir/vector-db-native

Native Node.js bindings for Fabstir Vector Database.

## Installation

```bash
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz
```

## Quick Start

```javascript
const { VectorDBSession } = require('@fabstir/vector-db-native');

async function main() {
  const session = await VectorDBSession.create({
    s5Portal: 'http://localhost:5524',
    userSeedPhrase: 'your-seed-phrase',
    sessionId: 'session-123',
  });

  try {
    // Add vectors
    await session.addVectors([
      {
        id: 'doc1',
        vector: [...], // 384-dim embedding
        metadata: { text: 'Hello world' }
      }
    ]);

    // Search
    const results = await session.search(queryVector, 5);
    console.log(results);
  } finally {
    await session.destroy();
  }
}
```

## API

See `index.d.ts` for full TypeScript definitions.

## Requirements

- Node.js >= 16
- Linux x64 or ARM64

## License

MIT
```

---

## Implementation Checklist

**Approach:** Strict TDD Bounded Autonomy - Each phase must complete with passing tests before proceeding

### Phase 1: Setup ✅ COMPLETE
- [x] Create directory structure
- [x] Configure Cargo workspace
- [x] Set up napi-rs dependencies
- [x] Create package.json with build scripts
- [x] Auto-generated TypeScript definitions

### Phase 2: Minimal Bindings Implementation ✅ COMPLETE
- [x] Implement error types (bindings/node/src/error.rs)
- [x] Implement type conversions (bindings/node/src/types.rs)
- [x] Implement minimal VectorDBSession
  - [x] create() factory
  - [x] addVectors() with metadata HashMap
  - [x] search() with metadata retrieval
  - [x] getStats()
  - [x] destroy()
  - [x] loadUserVectors() - stub (throws "not implemented")
  - [x] saveToS5() - stub (throws "not implemented")
- [x] Build successful (18MB tarball created)
- [x] Updated VECTOR_DB_INTEGRATION.md to reflect Phase 2 limitations

**Status:** Delivered in-memory-only bindings to continue development

---

### Phase 3: HybridIndex Serialization ✅ COMPLETE

**Goal:** Enable HybridIndex to serialize/deserialize for S5 persistence

**Status:** All sub-phases complete with 8/8 tests passing

#### 3.1: Create Persistence Module ✅ COMPLETE
- [x] Create `src/hybrid/persistence.rs`
- [x] Define `PersistenceError` enum (follow HNSW/IVF pattern)
- [x] Define `HybridMetadata` struct with:
  - [x] version: u32
  - [x] config: HybridConfig
  - [x] recent_count: usize
  - [x] historical_count: usize
  - [x] total_vectors: usize
  - [x] timestamp: DateTime<Utc>
- [x] Implement `HybridMetadata::from_index(index: &HybridIndex)`
- [x] Implement `HybridMetadata::to_cbor() -> Result<Vec<u8>>`
- [x] Implement `HybridMetadata::from_cbor(data: &[u8]) -> Result<Self>`
- [x] Add serde derives to HybridConfig
- [x] Tests pass: `cargo test hybrid::persistence::tests` (3/3 passed)

#### 3.2: Implement Serializable Structs ✅ COMPLETE
- [x] Create `SerializableTimestamps` struct
  - [x] `timestamps: HashMap<VectorId, DateTime<Utc>>`
  - [x] `to_cbor()` method
  - [x] `from_cbor()` method
- [x] Tests pass (included in Phase 3.1 tests)

#### 3.3: Create HybridPersister ✅ COMPLETE
- [x] Create `HybridPersister<S: S5Storage + Clone>` struct
- [x] Implement `new(storage: S)` constructor
- [x] Implement `save_index(&self, index: &HybridIndex, path: &str)`
  - [x] Save metadata to `{path}/metadata.cbor`
  - [x] Save timestamps to `{path}/timestamps.cbor`
  - [x] Use HNSWPersister to save recent index to `{path}/recent/`
  - [x] Use IVFPersister to save historical index to `{path}/historical/`
- [x] Implement `load_index(&self, path: &str) -> Result<HybridIndex>`
  - [x] Load metadata from `{path}/metadata.cbor`
  - [x] Load timestamps from `{path}/timestamps.cbor`
  - [x] Use HNSWPersister to load recent index from `{path}/recent/`
  - [x] Use IVFPersister to load historical index from `{path}/historical/`
  - [x] Reconstruct HybridIndex with loaded data
- [x] Build successful with no errors

#### 3.4: Update HybridIndex Core ✅ COMPLETE
- [x] Update `src/hybrid/mod.rs` to export persistence module
- [x] Add `get_timestamps()` accessor method to HybridIndex
- [x] Add `get_recent_index()` accessor method to HybridIndex
- [x] Add `get_historical_index()` accessor method to HybridIndex
- [x] Add `from_parts()` constructor to HybridIndex for deserialization
- [x] Add HashMap import to hybrid/core.rs
- [x] Build successful with no errors

#### 3.5: Write Tests for Serialization (TDD) ✅ COMPLETE
- [x] Tests added to existing module in `src/hybrid/persistence.rs`
- [x] Test: Metadata serialization round-trip ✅
- [x] Test: Timestamps serialization round-trip ✅
- [x] Test: HybridIndex save and load with MockS5Storage ✅
- [x] Test: HybridIndex serialization preserves vector count (20 vectors) ✅
- [x] Test: HybridIndex serialization preserves search quality (distances) ✅
- [x] Test: Empty index save/load ✅
- [x] Test: Missing metadata error handling ✅
- [x] Test: Version compatibility check ✅
- [x] Run tests: `cargo test hybrid::persistence --lib`
- [x] **All 8 persistence tests passing** ✅

**Test Results:**
```
running 8 tests
test hybrid::persistence::tests::test_hybrid_metadata_cbor_roundtrip ... ok
test hybrid::persistence::tests::test_hybrid_persister_missing_metadata ... ok
test hybrid::persistence::tests::test_hybrid_persister_empty_index ... ok
test hybrid::persistence::tests::test_version_compatibility ... ok
test hybrid::persistence::tests::test_serializable_timestamps_cbor_roundtrip ... ok
test hybrid::persistence::tests::test_hybrid_persister_save_and_load ... ok
test hybrid::persistence::tests::test_hybrid_persister_preserves_search_results ... ok
test hybrid::persistence::tests::test_hybrid_persister_preserves_vector_count ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured
```

---

### Phase 4: Update Node Bindings with Real S5 Integration 🔧 PENDING

**Goal:** Replace stubs with real S5 persistence using HybridIndex serialization

#### 4.1: Update Type Definitions ✅ COMPLETE
- [x] Change `metadata: String` to `metadata: serde_json::Value` in types.rs
- [x] Update VectorInput struct
- [x] Update SearchResult struct
- [x] Rebuild to verify TypeScript definitions updated

**Completed:** Metadata fields now use `serde_json::Value` instead of JSON strings. TypeScript definitions show `metadata: any` allowing native JavaScript objects. Added `serde-json` feature to napi dependency.

#### 4.2: Integrate EnhancedS5Storage
- [ ] Update SessionState in session.rs:
  - [ ] Remove standalone metadata HashMap
  - [ ] Add `storage: Arc<EnhancedS5Storage>`
- [ ] Update `create()` factory:
  - [ ] Initialize EnhancedS5Storage with config
  - [ ] Add error handling for S5 connection
- [ ] Update `addVectors()`:
  - [ ] Remove JSON.stringify requirement
  - [ ] Pass metadata directly as serde_json::Value
- [ ] Update `search()`:
  - [ ] Return metadata as serde_json::Value
  - [ ] Remove JSON.parse requirement

#### 4.3: Implement Real loadUserVectors()
- [ ] Remove "not implemented" error
- [ ] Add `persister: HybridPersister<EnhancedS5Storage>`
- [ ] Call `persister.load_index(cid)` to load from S5
- [ ] Replace current index with loaded index
- [ ] Handle lazy loading option
- [ ] Add comprehensive error handling

#### 4.4: Implement Real saveToS5()
- [ ] Remove "not implemented" error
- [ ] Call `persister.save_index(&index, session_id)`
- [ ] Get CID from S5 storage
- [ ] Return CID string
- [ ] Add comprehensive error handling

---

### Phase 5: Comprehensive Testing (TDD) 🧪 PENDING

**Goal:** Ensure all features work correctly with 100% test coverage

#### 5.1: Write Unit Tests FIRST (Before Running)
- [ ] Create `bindings/node/test/session.test.js`
- [ ] Test: Session create with valid config
- [ ] Test: Session create with invalid config (missing fields)
- [ ] Test: addVectors with metadata objects (not strings)
- [ ] Test: search returns metadata objects (not strings)
- [ ] Test: getStats returns accurate counts
- [ ] Test: destroy() clears memory
- [ ] Test: Operations after destroy() throw errors

#### 5.2: Write Integration Tests with MockS5Storage
- [ ] Create `bindings/node/test/s5-integration.test.js`
- [ ] Test: saveToS5() returns valid CID
- [ ] Test: loadUserVectors() loads from CID
- [ ] Test: Round-trip save and load preserves vectors
- [ ] Test: Round-trip save and load preserves metadata
- [ ] Test: Round-trip save and load preserves search results
- [ ] Test: Multiple sessions can load same CID

#### 5.3: Write Memory Leak Tests
- [ ] Test: destroy() actually frees memory
- [ ] Test: Multiple create/destroy cycles don't leak
- [ ] Test: Large dataset (10K vectors) cleanup

#### 5.4: Run All Tests
- [ ] Run unit tests: `cd bindings/node && npm test`
- [ ] All unit tests must pass ✅
- [ ] Run integration tests (with TEST_WITH_S5=true if S5 available)
- [ ] All integration tests must pass ✅
- [ ] Run memory tests
- [ ] All memory tests must pass ✅
- [ ] Fix any failures and re-run until 100% pass

**BLOCKER:** Cannot proceed to Phase 6 until all tests pass

---

### Phase 6: Production Build & Distribution 📦 PENDING

**Goal:** Create production-ready tarball for SDK developer

#### 6.1: Production Build
- [ ] Run `cargo build --release` in workspace root
- [ ] Run `npm run build` in bindings/node
- [ ] Verify binary size is reasonable
- [ ] Verify TypeScript definitions are current
- [ ] Run final test suite to confirm release build works

#### 6.2: Create Distribution Package
- [ ] Run `npm pack` to create tarball
- [ ] Verify tarball contents (index.js, index.d.ts, *.node, package.json, README.md)
- [ ] Test install from tarball in clean directory
- [ ] Verify installed package works

#### 6.3: Update Documentation
- [ ] Update bindings/node/README.md:
  - [ ] Remove "Phase 2 limitations" warnings
  - [ ] Update "What Works" section
  - [ ] Add S5 persistence examples
  - [ ] Mark loadUserVectors() and saveToS5() as working
- [ ] Update docs/sdk-reference/VECTOR_DB_INTEGRATION.md:
  - [ ] Remove implementation status banner
  - [ ] Update metadata examples (objects, not strings)
  - [ ] Remove JSON.stringify/parse from examples
  - [ ] Mark all functions as implemented
  - [ ] Add S5 persistence workflow examples
- [ ] Update CLAUDE.md Phase status

#### 6.4: Prepare for SDK Developer
- [ ] Write developer message with:
  - [ ] What's included in v0.1.0
  - [ ] Installation instructions
  - [ ] Breaking changes from Phase 2 (metadata is now objects)
  - [ ] Migration guide if they used Phase 2 version
- [ ] Include tarball path
- [ ] Include VECTOR_DB_INTEGRATION.md path
- [ ] Include quick start example

**DELIVERY:** Only deliver to SDK developer after Phase 6 complete

---

## Known Issues & TODOs

### Current Limitations

1. **Sync Stats:** `getStats()` is currently sync, should be async
   - Workaround: Use `try_read()` instead of blocking read
   - TODO: Make it properly async

2. **Error Context:** Need better error context
   - TODO: Add source file/line info to errors
   - TODO: Add structured error data

3. **Memory Tracking:** Memory budget not enforced yet
   - TODO: Implement memory tracking in HybridIndex
   - TODO: Add memory limits to prevent OOM

### Future Enhancements

- [ ] Streaming search results for large result sets
- [ ] Metadata filtering during search
- [ ] Background index optimization
- [ ] Incremental S5 saves (delta updates)
- [ ] Compression for S5 storage

---

## Performance Targets

| Metric | Target | Measured |
|--------|--------|----------|
| Session creation | < 100ms | TBD |
| Load 100K vectors (lazy) | < 5s | TBD |
| Search latency (p99) | < 50ms | TBD |
| Memory overhead | < 10% | TBD |
| S5 save time (100K vectors) | < 10s | TBD |

---

## Support

For implementation questions, see:
- napi-rs documentation: https://napi.rs
- Fabstir Vector DB core: ../../../README.md
- SDK integration guide: ../../docs/sdk-reference/VECTOR_DB_INTEGRATION.md

---

**Last Updated:** 2025-01-26
**Status:** Implementation Ready
