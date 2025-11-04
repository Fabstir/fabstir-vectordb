# WASM Bindings Implementation - Option A: Core Library Wrapper

## Project Overview

Implement production-ready WebAssembly bindings for Fabstir Vector DB by wrapping the existing Rust core library (HybridIndex, HNSW, IVF). This enables browser-based vector operations with full feature parity to the Node.js native bindings, including chunked storage, S5.js integration, metadata filtering, and CRUD operations.

**Target Version**: v0.3.0 (WASM bindings)
**Timeline**: 19-21 hours (~3 days)
**Breaking Changes**: None (new package: `@fabstir/vector-db-wasm`)

**Key Challenge**: Core library uses `tokio::sync::RwLock` and async operations which are incompatible with WASM's single-threaded synchronous execution model. Must replace with `std::sync::RwLock` and provide synchronous wrappers.

## Architecture Summary

### Target API Surface

The Node.js bindings (`bindings/node/src/session.rs`) demonstrate the exact API we need:

```javascript
// Target API (from Node.js bindings - proven)
const session = await VectorDbSession.create(config);
await session.addVectors([{ id, vector, metadata }]);
const results = await session.search(queryVector, k, { filter, threshold });
await session.deleteVector(id);
await session.deleteByMetadata(filter);
await session.updateMetadata(id, metadata);
const cid = await session.saveToS5();
await session.loadUserVectors(cid, options);
await session.vacuum();
await session.destroy();
```

### Tokio Compatibility Challenge

**Current Architecture** (from `src/hybrid/core.rs`, `src/hnsw/core.rs`, `src/ivf/core.rs`):
- Uses `tokio::sync::RwLock` for concurrent access (lines src/hybrid/core.rs:14)
- Uses `Arc<RwLock<T>>` pattern throughout (120 occurrences across 11 files)
- Async operations with `.await` for lock acquisition
- Node.js bindings use `#[napi]` with async functions

**WASM Constraints**:
- Single-threaded execution (no true concurrency)
- No tokio runtime available
- Must use `std::sync::RwLock` instead
- Can simulate async with `wasm-bindgen-futures` but no benefit for locks

**Solution Strategy**:
1. Create WASM-specific feature flag: `wasm-target`
2. Replace `tokio::sync::RwLock` with `std::sync::RwLock` in WASM builds
3. Provide synchronous wrappers around core operations
4. Use `wasm-bindgen-futures` only for S5 network operations

### Storage Architecture (from existing implementation)

**Existing Rust Core** (already supports):
- ✅ Chunked storage (`src/core/chunk.rs` - manifest v3)
- ✅ HybridIndex with HNSW + IVF (`src/hybrid/core.rs`)
- ✅ Soft deletion + vacuum (`src/hybrid/core.rs:843-929`)
- ✅ Metadata filtering (`src/core/metadata_filter.rs`)
- ✅ S5 storage abstraction (`src/core/storage.rs`)

**WASM-Specific Additions Needed**:
- JavaScript bridge for S5.js calls (network I/O)
- IndexedDB cache layer (browser local storage)
- WASM-compatible lock primitives

### Repository Structure

```
fabstir-ai-vector-db/
├── src/                           # Core library (EXISTING)
│   ├── hybrid/core.rs            # HybridIndex (tokio locks → std locks)
│   ├── hnsw/core.rs              # HNSWIndex (tokio locks → std locks)
│   ├── ivf/core.rs               # IVFIndex (tokio locks → std locks)
│   └── core/metadata_filter.rs   # Filter language (no changes needed)
├── bindings/
│   ├── node/                     # Node.js bindings (REFERENCE API)
│   │   └── src/session.rs        # Proven API surface (899 lines)
│   └── wasm/                     # WASM bindings (THIS PLAN)
│       ├── Cargo.toml            # wasm-bindgen + feature flags
│       ├── src/
│       │   ├── lib.rs            # Entry point + panic hook
│       │   ├── session.rs        # VectorDbSession (mirrors Node.js API)
│       │   ├── sync_wrapper.rs   # Tokio → std::sync adapter
│       │   ├── storage_s5.rs     # S5.js JavaScript bridge
│       │   ├── storage_idb.rs    # IndexedDB cache
│       │   └── types.rs          # JsValue conversions
│       ├── tests/
│       │   └── wasm.rs           # wasm-bindgen-test
│       └── pkg/                  # wasm-pack output
```

## Current Status

- ⏳ Phase 1: Tokio Compatibility Layer (0% - Not Started)
- ⏳ Phase 2: Core API Bindings (0% - Not Started)
- ⏳ Phase 3: S5.js Storage Integration (0% - Not Started)
- ⏳ Phase 4: IndexedDB Cache Layer (0% - Not Started)
- ⏳ Phase 5: Build & Package (0% - Not Started)
- ⏳ Phase 6: Examples & Documentation (0% - Not Started)

**Overall Progress**: 0% (0/6 phases complete)

## Implementation Phases

### Phase 1: Tokio Compatibility Layer (Day 1 - 4 hours)

**Goal**: Replace tokio async locks with std::sync locks for WASM compatibility

#### Sub-phase 1.1: Feature Flag Setup ⏳

**TDD Approach**: Write compilation tests first

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/feature_flags_test.rs` - 6 tests
  - [ ] test_wasm_target_feature_enabled()
  - [ ] test_std_rwlock_used_in_wasm()
  - [ ] test_tokio_rwlock_not_in_wasm_build()
  - [ ] test_async_removed_from_wasm_methods()
  - [ ] test_compilation_succeeds_wasm32()
  - [ ] test_compilation_succeeds_native()

**Implementation Tasks**:
- [ ] Add `wasm-target` feature to root `Cargo.toml` workspace
- [ ] Add conditional compilation in `src/hybrid/core.rs`:
  ```rust
  #[cfg(not(feature = "wasm-target"))]
  use tokio::sync::RwLock;
  #[cfg(feature = "wasm-target")]
  use std::sync::RwLock;
  ```
- [ ] Repeat for `src/hnsw/core.rs` (lines 1-15)
- [ ] Repeat for `src/ivf/core.rs` (lines 1-15)
- [ ] Update `src/hybrid/core.rs` struct fields (line ~208):
  - Change `Arc<tokio::sync::RwLock<T>>` → `Arc<std::sync::RwLock<T>>`
- [ ] Update lock acquisition (remove `.await`):
  - `index.read().await` → `index.read().unwrap()`
  - `index.write().await` → `index.write().unwrap()`
- [ ] Test with `cargo build --features wasm-target`

**Bounded Autonomy**: ~80 lines total across 3 core files (small targeted changes)

**Success Criteria**:
- [ ] WASM build compiles without tokio dependency
- [ ] Native build still uses tokio (no regression)
- [ ] All 6 compilation tests pass
- [ ] Zero clippy warnings

**Estimated Time**: 2 hours

---

#### Sub-phase 1.2: Synchronous API Wrappers ⏳

**TDD Approach**: Write unit tests for sync wrappers

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/sync_wrapper_test.rs` - 10 tests
  - [ ] test_hybrid_index_insert_sync()
  - [ ] test_hybrid_index_search_sync()
  - [ ] test_hybrid_index_delete_sync()
  - [ ] test_hnsw_operations_sync()
  - [ ] test_ivf_operations_sync()
  - [ ] test_no_deadlocks_single_thread()
  - [ ] test_error_handling_sync()
  - [ ] test_concurrent_reads_safe()
  - [ ] test_metadata_access_sync()
  - [ ] test_persistence_operations_sync()

**Implementation Tasks**:
- [ ] Create `bindings/wasm/src/sync_wrapper.rs` (~150 lines)
- [ ] Implement `SyncHybridIndex` wrapper:
  ```rust
  pub struct SyncHybridIndex {
      inner: Arc<RwLock<HybridIndex>>,
  }

  impl SyncHybridIndex {
      pub fn new(config: HybridConfig) -> Self { /* ... */ }
      pub fn insert(&self, id: VectorId, vector: Vec<f32>) -> Result<()> {
          let mut guard = self.inner.write().unwrap();
          guard.insert_sync(id, vector) // New sync method
      }
      pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> { /* ... */ }
      pub fn delete(&self, id: VectorId) -> Result<()> { /* ... */ }
      // ... mirror all HybridIndex methods
  }
  ```
- [ ] Add `*_sync()` methods to `HybridIndex`:
  - `insert_sync()` - no await, returns immediately
  - `search_sync()` - synchronous search
  - `delete_sync()` - synchronous deletion
- [ ] Update `src/hybrid/core.rs` with sync variants (~60 lines added)
- [ ] Ensure no `.await` in sync methods (WASM single-threaded)

**Bounded Autonomy**: 150 lines sync_wrapper.rs + 60 lines core modifications

**Success Criteria**:
- [ ] All sync wrappers compile for wasm32-unknown-unknown
- [ ] No async/await in sync methods
- [ ] All 10 unit tests pass
- [ ] Lock guards dropped correctly (no deadlocks)

**Estimated Time**: 2 hours

---

### Phase 2: Core API Bindings (Day 2 - 4 hours)

**Goal**: Expose HybridIndex operations to JavaScript via wasm-bindgen

#### Sub-phase 2.1: VectorDbSession Structure ⏳

**TDD Approach**: Write WASM integration tests first

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/session_api_test.rs` - 12 tests
  - [ ] test_session_create_with_config()
  - [ ] test_session_create_validates_config()
  - [ ] test_add_single_vector()
  - [ ] test_add_vectors_batch()
  - [ ] test_search_returns_results()
  - [ ] test_delete_vector()
  - [ ] test_delete_by_metadata()
  - [ ] test_update_metadata()
  - [ ] test_get_stats()
  - [ ] test_vacuum_operation()
  - [ ] test_session_destroy()
  - [ ] test_dimension_validation()

**Implementation Tasks**:
- [ ] Create `bindings/wasm/Cargo.toml` with dependencies:
  ```toml
  [dependencies]
  wasm-bindgen = "0.2"
  wasm-bindgen-futures = "0.4"
  js-sys = "0.3"
  web-sys = { version = "0.3", features = ["Window", "console"] }
  serde-wasm-bindgen = "0.6"
  vector-db = { path = "../../", features = ["wasm-target"] }
  ```
- [ ] Create `bindings/wasm/src/lib.rs` (entry point, ~30 lines)
- [ ] Create `bindings/wasm/src/types.rs` (~120 lines):
  - `VectorInput` struct (mirrors Node.js)
  - `SearchResult` struct
  - `DeleteResult` struct
  - `VacuumStats` struct
  - `SessionStats` struct
  - JsValue conversion traits
- [ ] Create `bindings/wasm/src/session.rs` (~400 lines - mirrors Node.js session.rs)
- [ ] Implement `#[wasm_bindgen] pub struct VectorDbSession`:
  ```rust
  #[wasm_bindgen]
  pub struct VectorDbSession {
      session_id: String,
      index: SyncHybridIndex,
      metadata: HashMap<String, serde_json::Value>,
      storage: WasmS5Storage,
      config: VectorDBConfig,
  }
  ```

**Bounded Autonomy**: 30 lines lib.rs + 120 lines types.rs + 400 lines session.rs = 550 lines

**Success Criteria**:
- [ ] `VectorDbSession::create(config)` compiles
- [ ] All struct fields accessible
- [ ] TypeScript definitions auto-generated
- [ ] 12 integration tests pass

**Estimated Time**: 2 hours

---

#### Sub-phase 2.2: CRUD Operations API ⏳

**TDD Approach**: Mirror Node.js API exactly, test each method

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/crud_operations_test.rs` - 15 tests
  - [ ] test_add_vectors_with_metadata()
  - [ ] test_add_validates_dimensions()
  - [ ] test_search_basic()
  - [ ] test_search_with_filter()
  - [ ] test_search_with_threshold()
  - [ ] test_delete_vector_by_id()
  - [ ] test_delete_by_metadata_filter()
  - [ ] test_update_metadata_preserves_id()
  - [ ] test_update_nonexistent_vector_errors()
  - [ ] test_get_stats_accurate()
  - [ ] test_vacuum_removes_deleted()
  - [ ] test_crud_roundtrip()
  - [ ] test_metadata_filter_complex()
  - [ ] test_error_handling()
  - [ ] test_jsvalue_conversions()

**Implementation Tasks**:
- [ ] Add methods to `VectorDbSession` (in session.rs):
  - `#[wasm_bindgen] pub fn add_vectors(&mut self, vectors: JsValue) -> Result<(), JsValue>`
  - `#[wasm_bindgen] pub fn search(&self, query: Vec<f32>, k: u32, options: JsValue) -> Result<JsValue, JsValue>`
  - `#[wasm_bindgen] pub fn delete_vector(&mut self, id: String) -> Result<(), JsValue>`
  - `#[wasm_bindgen] pub fn delete_by_metadata(&mut self, filter: JsValue) -> Result<JsValue, JsValue>`
  - `#[wasm_bindgen] pub fn update_metadata(&mut self, id: String, metadata: JsValue) -> Result<(), JsValue>`
  - `#[wasm_bindgen] pub fn get_stats(&self) -> Result<JsValue, JsValue>`
  - `#[wasm_bindgen] pub fn vacuum(&mut self) -> Result<JsValue, JsValue>`
  - `#[wasm_bindgen] pub fn destroy(self) -> Result<(), JsValue>`
- [ ] Reference `bindings/node/src/session.rs` for exact logic (lines 338-821)
- [ ] Implement JsValue ↔ Rust type conversions:
  - `parse_vector_inputs()` helper
  - `serialize_search_results()` helper
  - `parse_search_options()` helper
  - `parse_metadata_filter()` helper
- [ ] Add error handling with descriptive JsValue messages

**Bounded Autonomy**: ~300 lines added to session.rs (CRUD methods + helpers)

**Success Criteria**:
- [ ] All 8 CRUD methods exposed to JS
- [ ] No panics (only Result<T, JsValue>)
- [ ] Error messages match Node.js bindings
- [ ] All 15 tests pass

**Estimated Time**: 2 hours

---

### Phase 3: S5.js Storage Integration (Day 3 - 3 hours)

**Goal**: Enable persistence to S5 decentralized storage via JavaScript bridge

#### Sub-phase 3.1: S5.js JavaScript Bridge ⏳

**TDD Approach**: Mock S5 responses, test JavaScript interop

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/s5_integration_test.rs` - 12 tests
  - [ ] test_s5_upload_manifest()
  - [ ] test_s5_upload_chunks()
  - [ ] test_s5_download_manifest()
  - [ ] test_s5_download_chunks()
  - [ ] test_s5_returns_cid()
  - [ ] test_s5_handles_network_errors()
  - [ ] test_s5_retry_logic()
  - [ ] test_s5_chunked_upload()
  - [ ] test_s5_concurrent_downloads()
  - [ ] test_s5_js_bridge_available()
  - [ ] test_s5_encryption_enabled()
  - [ ] test_s5_large_dataset()

**Implementation Tasks**:
- [ ] Create `bindings/wasm/js/s5_bridge.js` (~100 lines):
  ```javascript
  import { S5Client } from '@fabstir/s5js';

  export async function s5Upload(path, data) {
    const client = new S5Client(config.portal);
    const cid = await client.uploadFile(path, data);
    return cid;
  }

  export async function s5Download(cid) {
    const client = new S5Client(config.portal);
    const data = await client.downloadFile(cid);
    return data;
  }
  ```
- [ ] Create `bindings/wasm/src/storage_s5.rs` (~250 lines):
  ```rust
  #[wasm_bindgen(module = "/js/s5_bridge.js")]
  extern "C" {
      #[wasm_bindgen(catch)]
      async fn s5Upload(path: &str, data: &[u8]) -> Result<JsValue, JsValue>;

      #[wasm_bindgen(catch)]
      async fn s5Download(cid: &str) -> Result<JsValue, JsValue>;
  }

  pub struct WasmS5Storage {
      portal_url: String,
      seed_phrase: String,
  }

  impl WasmS5Storage {
      pub async fn save_to_s5(&self, index: &HybridIndex, session_id: &str) -> Result<String, JsValue> {
          // Serialize index + metadata
          // Upload manifest, chunks, metadata_map
          // Return CID
      }

      pub async fn load_from_s5(&self, cid: &str) -> Result<HybridIndex, JsValue> {
          // Download manifest
          // Download chunks in parallel
          // Reconstruct index
      }
  }
  ```
- [ ] Implement `save_to_s5()` method:
  - Serialize HybridIndex to manifest + chunks (use existing `HybridPersister`)
  - Upload manifest.json to S5 via JS bridge
  - Upload chunks in parallel (5-10 concurrent)
  - Upload metadata_map.cbor
  - Return session_id as CID
- [ ] Implement `load_from_s5()` method:
  - Download manifest from CID
  - Parse chunk list
  - Download chunks in parallel
  - Reconstruct HybridIndex
  - Load metadata HashMap
- [ ] Add retry logic (3 attempts, exponential backoff)
- [ ] Handle S5 errors gracefully (network timeout, invalid CID)

**Bounded Autonomy**: 100 lines s5_bridge.js + 250 lines storage_s5.rs = 350 lines

**Success Criteria**:
- [ ] Vectors persist to S5 network (real or mock)
- [ ] CID returned and usable for reload
- [ ] All 12 S5 integration tests pass
- [ ] Chunked upload/download works
- [ ] Errors propagate to JS with clear messages

**Estimated Time**: 3 hours

---

### Phase 4: IndexedDB Cache Layer (Day 4 - 3 hours)

**Goal**: Add local browser storage to avoid S5 re-downloads on app restart

#### Sub-phase 4.1: IndexedDB Storage Implementation ⏳

**TDD Approach**: Test persistence across page reloads

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/indexeddb_test.rs` - 10 tests
  - [ ] test_idb_database_initialization()
  - [ ] test_idb_save_manifest()
  - [ ] test_idb_load_manifest()
  - [ ] test_idb_save_chunks()
  - [ ] test_idb_load_chunks()
  - [ ] test_idb_clear_database()
  - [ ] test_idb_quota_exceeded_handling()
  - [ ] test_idb_persists_across_reload()
  - [ ] test_idb_version_upgrade()
  - [ ] test_idb_fallback_unavailable()

**Implementation Tasks**:
- [ ] Add web-sys features to `Cargo.toml`:
  ```toml
  web-sys = { version = "0.3", features = [
    "IdbDatabase", "IdbObjectStore", "IdbTransaction",
    "IdbRequest", "IdbVersionChangeEvent"
  ]}
  ```
- [ ] Create `bindings/wasm/src/storage_idb.rs` (~300 lines):
  ```rust
  pub struct IndexedDBCache {
      db_name: String,
  }

  impl IndexedDBCache {
      pub async fn init(&self) -> Result<(), JsValue> {
          // Open database "fabstir-vector-db"
          // Create object stores: "manifests", "chunks", "metadata"
      }

      pub async fn save_manifest(&self, cid: &str, manifest: &[u8]) -> Result<(), JsValue> {
          // Save to "manifests" store with key=cid
      }

      pub async fn load_manifest(&self, cid: &str) -> Result<Option<Vec<u8>>, JsValue> {
          // Load from "manifests" store
      }

      pub async fn save_chunk(&self, cid: &str, chunk_id: usize, data: &[u8]) -> Result<(), JsValue> {
          // Save to "chunks" store with key=`${cid}/${chunk_id}`
      }

      pub async fn load_chunk(&self, cid: &str, chunk_id: usize) -> Result<Option<Vec<u8>>, JsValue> {
          // Load from "chunks" store
      }

      pub async fn clear(&self) -> Result<(), JsValue> {
          // Clear all object stores
      }
  }
  ```
- [ ] Integrate with `WasmS5Storage`:
  - Check IDB cache before downloading from S5
  - Save to IDB after downloading from S5
  - LRU eviction if quota exceeded (store last 5 CIDs)
- [ ] Handle IndexedDB errors:
  - Quota exceeded → evict oldest CID
  - Private browsing mode → disable IDB gracefully
  - Version mismatch → delete and recreate

**Bounded Autonomy**: 300 lines storage_idb.rs

**Success Criteria**:
- [ ] IndexedDB stores manifests and chunks
- [ ] Data persists across browser tab closes
- [ ] Quota errors handled gracefully
- [ ] All 10 IDB tests pass
- [ ] Works in Chrome, Firefox, Safari

**Estimated Time**: 3 hours

---

### Phase 5: Build & Package (Day 5 - 2 hours)

**Goal**: Configure wasm-pack for production builds and npm packaging

#### Sub-phase 5.1: Build Configuration ⏳

**TDD Approach**: Validate build outputs

**Test Files** (TDD - Written First):
- [ ] `bindings/wasm/tests/build_validation.sh` - 8 tests
  - [ ] test_wasm_pack_build_succeeds()
  - [ ] test_pkg_directory_created()
  - [ ] test_wasm_file_size_under_500kb()
  - [ ] test_gzipped_size_under_200kb()
  - [ ] test_js_glue_generated()
  - [ ] test_typescript_defs_generated()
  - [ ] test_package_json_valid()
  - [ ] test_wasm_opt_applied()

**Implementation Tasks**:
- [ ] Configure `Cargo.toml` for wasm-pack:
  ```toml
  [lib]
  crate-type = ["cdylib", "rlib"]

  [package.metadata.wasm-pack.profile.release]
  wasm-opt = ["-Oz", "--enable-simd"]

  [profile.release]
  opt-level = "z"
  lto = true
  codegen-units = 1
  ```
- [ ] Create `.cargo/config.toml`:
  ```toml
  [target.wasm32-unknown-unknown]
  rustflags = ["-C", "link-arg=-s"]
  ```
- [ ] Create build script `bindings/wasm/build.sh`:
  ```bash
  #!/bin/bash
  wasm-pack build --target web --out-dir pkg --release
  wasm-opt -Oz pkg/vector_db_wasm_bg.wasm -o pkg/vector_db_wasm_bg.wasm
  ```
- [ ] Test build: `./build.sh`
- [ ] Verify bundle size:
  - WASM file: < 500 KB uncompressed
  - Gzipped: < 200 KB
- [ ] Check TypeScript definitions exported

**Bounded Autonomy**: 3 config files + build script = ~50 lines

**Success Criteria**:
- [ ] Build completes successfully
- [ ] Bundle size under 500 KB
- [ ] TypeScript definitions accurate
- [ ] All 8 build tests pass

**Estimated Time**: 1 hour

---

#### Sub-phase 5.2: NPM Package Configuration ⏳

**TDD Approach**: Test npm pack output

**Implementation Tasks**:
- [ ] Create `bindings/wasm/package.json`:
  ```json
  {
    "name": "@fabstir/vector-db-wasm",
    "version": "0.3.0",
    "description": "WebAssembly bindings for Fabstir Vector DB",
    "main": "pkg/vector_db_wasm.js",
    "types": "pkg/vector_db_wasm.d.ts",
    "files": [
      "pkg/*.js",
      "pkg/*.wasm",
      "pkg/*.d.ts",
      "js/s5_bridge.js"
    ],
    "dependencies": {
      "@fabstir/s5js": "^0.9.0"
    },
    "peerDependencies": {},
    "keywords": ["vector-database", "wasm", "s5", "decentralized"],
    "license": "BUSL-1.1"
  }
  ```
- [ ] Create README with usage examples
- [ ] Add example: `bindings/wasm/examples/browser_demo.html`
- [ ] Test `npm pack` → verify tarball contents
- [ ] Test `npm publish --dry-run`

**Bounded Autonomy**: 1 package.json + README + example = ~150 lines

**Success Criteria**:
- [ ] Package includes all necessary files
- [ ] Dependencies correct
- [ ] Example runs in browser
- [ ] npm pack succeeds

**Estimated Time**: 1 hour

---

### Phase 6: Examples & Documentation (Day 6 - 3 hours)

**Goal**: Create comprehensive examples and documentation for SDK developer integration

#### Sub-phase 6.1: Working Examples ⏳

**TDD Approach**: Create runnable examples that demonstrate all features

**Implementation Tasks**:
- [ ] Create `bindings/wasm/examples/` directory
- [ ] Create `bindings/wasm/examples/01-basic-usage.html` (~150 lines):
  ```html
  <!DOCTYPE html>
  <html>
  <head><title>Basic Vector DB Usage</title></head>
  <body>
    <script type="module">
      import init, { VectorDbSession } from '../pkg/vector_db_wasm.js';

      async function demo() {
        await init();

        // Create session
        const session = await VectorDbSession.create({
          s5Portal: 'http://localhost:5522',
          userSeedPhrase: 'demo-seed-phrase',
          sessionId: 'demo-session',
          dimension: 384
        });

        // Add vectors
        await session.addVectors([
          { id: 'doc1', vector: [...], metadata: { text: 'Hello' } },
          { id: 'doc2', vector: [...], metadata: { text: 'World' } }
        ]);

        // Search
        const results = await session.search(queryVector, 5);
        console.log('Results:', results);

        // Cleanup
        await session.destroy();
      }

      demo().catch(console.error);
    </script>
  </body>
  </html>
  ```
- [ ] Create `bindings/wasm/examples/02-crud-operations.html` (~200 lines):
  - Add, search, delete, update operations
  - Metadata filtering examples
  - Error handling patterns
- [ ] Create `bindings/wasm/examples/03-s5-persistence.html` (~180 lines):
  - Save to S5 network
  - Load from S5 by CID
  - IndexedDB caching demonstration
  - Offline mode handling
- [ ] Create `bindings/wasm/examples/04-advanced-search.html` (~220 lines):
  - Complex metadata filters (AND, OR, Range, In)
  - Threshold-based filtering
  - Large dataset examples (1000+ vectors)
  - Performance monitoring
- [ ] Create `bindings/wasm/examples/05-react-integration.jsx` (~250 lines):
  - React hooks for vector operations
  - useVectorDB() custom hook
  - Error boundaries
  - Loading states
- [ ] Create `bindings/wasm/examples/README.md` (~100 lines):
  - How to run each example
  - Prerequisites (S5 node, web server)
  - Troubleshooting common issues

**Bounded Autonomy**: 1100 lines total across 6 example files

**Success Criteria**:
- [ ] All 5 HTML examples load without errors
- [ ] React example shows proper TypeScript types
- [ ] Examples cover all major API features
- [ ] Each example has inline comments explaining key concepts
- [ ] README provides clear setup instructions

**Estimated Time**: 1.5 hours

---

#### Sub-phase 6.2: Integration Documentation ⏳

**TDD Approach**: Documentation-driven development (write docs that SDK developer needs)

**Implementation Tasks**:
- [ ] Create `bindings/wasm/docs/INTEGRATION_GUIDE.md` (~400 lines):
  - **Installation**: npm install, CDN usage, bundler setup
  - **Quick Start**: 5-minute integration guide
  - **Configuration**: All config options explained
  - **API Reference**: Every method with TypeScript signatures
  - **Error Handling**: All possible errors and how to handle them
  - **Performance Tips**: Bundle size, lazy loading, caching strategies
  - **Browser Compatibility**: Tested browsers, known issues, polyfills
  - **Troubleshooting**: Common errors and solutions
- [ ] Create `bindings/wasm/docs/MIGRATION_FROM_NODE.md` (~200 lines):
  - **API Differences**: Node.js vs WASM binding differences
  - **Code Migration**: Side-by-side comparison examples
  - **Storage Differences**: Node.js (file system) vs WASM (IndexedDB + S5)
  - **Performance Expectations**: Speed comparisons, memory usage
  - **When to Use WASM vs Node**: Decision tree
- [ ] Create `bindings/wasm/docs/API_REFERENCE.md` (~300 lines):
  - **VectorDbSession Class**: All methods with signatures
  - **Config Types**: VectorDBConfig, SearchOptions, DeleteResult, etc.
  - **Metadata Filters**: Complete filter language reference
  - **Error Types**: All error variants and when they occur
  - **TypeScript Types**: Full type definitions
- [ ] Create `bindings/wasm/docs/ARCHITECTURE.md` (~250 lines):
  - **WASM vs Node.js**: Why WASM for browsers
  - **Tokio Compatibility**: How we solved async/sync issue
  - **Storage Layer**: S5.js + IndexedDB architecture
  - **Memory Management**: WASM memory model, cleanup importance
  - **Security**: Client-side encryption, seed phrase handling
- [ ] Update `bindings/wasm/README.md` (~150 lines):
  - Overview with feature highlights
  - Quick start code snippet
  - Links to all documentation
  - Examples directory reference
  - Installation instructions
  - Browser compatibility matrix

**Bounded Autonomy**: 1300 lines total across 5 documentation files

**Success Criteria**:
- [ ] SDK developer can integrate without asking questions
- [ ] All API methods documented with examples
- [ ] Migration guide covers common scenarios
- [ ] Architecture doc explains design decisions
- [ ] README is clear and concise

**Estimated Time**: 1.5 hours

---

## Success Criteria

**Functional Requirements (MVP - Must Have)**:
- [ ] `VectorDbSession.create(config)` initializes WASM session
- [ ] `addVectors()` stores vectors with metadata
- [ ] `search()` returns nearest neighbors with filtering
- [ ] `deleteVector()` / `deleteByMetadata()` remove vectors
- [ ] `updateMetadata()` updates metadata without re-indexing
- [ ] `saveToS5()` persists to decentralized storage (returns CID)
- [ ] `loadUserVectors(cid)` restores from S5
- [ ] `vacuum()` physically removes deleted vectors
- [ ] IndexedDB cache avoids S5 re-downloads
- [ ] API matches Node.js bindings exactly

**Code Quality**:
- [ ] All 73 tests pass (unit + integration)
- [ ] Test coverage > 80%
- [ ] No panics in WASM (only Result<T, JsValue>)
- [ ] Zero clippy warnings
- [ ] TypeScript definitions complete and accurate
- [ ] Documentation with examples (Phase 6 deliverables)

**Performance Requirements**:
- [ ] Bundle size < 500 KB (WASM + JS glue)
- [ ] Gzipped size < 200 KB
- [ ] Search latency < 100ms for 10K vectors
- [ ] Memory usage < 150 MB for 10K vectors
- [ ] Startup time < 2s (with IndexedDB cache)
- [ ] IndexedDB save/load < 500ms

**Browser Compatibility**:
- [ ] Chrome 90+ ✅
- [ ] Firefox 88+ ✅
- [ ] Safari 14+ ✅
- [ ] Edge 90+ ✅
- [ ] Mobile Chrome Android ✅
- [ ] Mobile Safari iOS ✅

## Development Guidelines

### Do's ✅
- Write tests BEFORE implementation (strict TDD)
- Reference Node.js bindings (`bindings/node/src/session.rs`) for API logic
- Use `Result<T, JsValue>` for all error handling (no panics!)
- Use `std::sync::RwLock` instead of `tokio::sync::RwLock`
- Test in real browsers (not just Node.js)
- Document all public APIs with JSDoc
- Handle offline mode gracefully (IndexedDB fallback)
- Log errors to console (not panics)

### Don'ts ❌
- Never skip TDD (tests first, always!)
- Never panic in WASM (browsers can't recover)
- Never use `tokio` in WASM builds
- Never expose raw pointers to JavaScript
- Never block browser main thread (use workers if needed)
- Never skip bundle size optimization
- Never assume network is always available
- Never log user vectors/metadata (privacy!)

## Timeline Estimate

| Phase | Time | Deliverables |
|-------|------|--------------|
| Phase 1: Tokio Compatibility Layer | 4h | 16 tests |
| Phase 2: Core API Bindings | 4h | 27 tests |
| Phase 3: S5.js Storage Integration | 3h | 12 tests |
| Phase 4: IndexedDB Cache Layer | 3h | 10 tests |
| Phase 5: Build & Package | 2h | 8 tests |
| Phase 6: Examples & Documentation | 3h | 5 examples + 5 docs |
| **TOTAL** | **19 hours** | **73 tests + 10 files** |

**Buffer**: Add 10% for unexpected issues → **21 hours total (~3 days)**

**Recommended Approach**: Ship MVP (Phases 1-5) first (~16 hours), deliver Phase 6 documentation package for SDK developer handoff (~3 hours).

## Risk Mitigation

**Tokio Compatibility Risk**:
- **Mitigation**: Feature flag approach allows native builds to keep tokio
- **Mitigation**: Extensive compilation tests (Phase 1.1)
- **Mitigation**: Reference Node.js bindings for proven API patterns
- **Fallback**: If std::sync::RwLock has issues, use `parking_lot::RwLock`

**Bundle Size Risk**:
- **Mitigation**: Aggressive wasm-opt optimization (-Oz)
- **Mitigation**: Link-time optimization (LTO)
- **Mitigation**: Remove unused features (careful dependency selection)
- **Fallback**: Code splitting if > 500 KB

**Browser Compatibility Risk**:
- **Mitigation**: Test in all major browsers (Chrome, Firefox, Safari)
- **Mitigation**: Polyfills for IndexedDB if needed
- **Mitigation**: Graceful degradation (disable IDB cache if unavailable)
- **Fallback**: Provide Node.js WASM build as alternative

**Performance Risk**:
- **Mitigation**: Benchmark against Node.js bindings (target: 2x slower acceptable)
- **Mitigation**: Use SIMD if available (wasm-opt --enable-simd)
- **Mitigation**: Chunked loading (lazy load vectors on demand)
- **Fallback**: Add Web Worker offloading if main thread blocked

## Notes & Decisions

### Decision Log

**2025-11-04**: Chose Option A (core library wrapper) over Option B (standalone):
- Rationale: Full feature parity with Node.js bindings (v0.2.0 CRUD)
- Approach: Replace tokio locks with std::sync, synchronous wrappers
- Trade-off: More initial work (~18h vs ~12h), but production-ready output
- Future: No code duplication, easier to maintain

**2025-11-04**: Chose feature flag approach for tokio compatibility:
- Rationale: Native builds keep tokio (no performance regression)
- Approach: `#[cfg(feature = "wasm-target")]` conditional compilation
- Trade-off: Slightly more complex build, but zero runtime overhead
- Future: Easy to extend for other targets (embedded, mobile)

**2025-11-04**: Chose IndexedDB for browser cache:
- Rationale: Standard API, works offline, persists across tab closes
- Approach: LRU eviction when quota exceeded
- Trade-off: Async API complexity, but essential for UX
- Future: Add Web Storage API fallback for smaller datasets

**2025-11-04**: Chose S5.js JavaScript bridge over WASM HTTP:
- Rationale: S5.js already battle-tested, handles encryption
- Approach: `#[wasm_bindgen(module = "/js/s5_bridge.js")]`
- Trade-off: Requires S5.js as peer dependency, but avoids reinventing wheel
- Future: Easy to swap S5 implementation without touching Rust

### Open Questions

- [ ] Should we use Web Workers for search offloading?
  - Pro: Avoids blocking main thread on large datasets
  - Con: Adds complexity, SharedArrayBuffer issues
  - Decision: Phase 7 (v0.3.1) if needed

- [ ] Should we support Node.js WASM (in addition to native)?
  - Pro: Single binary for all platforms
  - Con: Slower than native bindings
  - Decision: Defer to v0.4.0

- [ ] Should we add LocalStorage fallback for IndexedDB?
  - Pro: Works in older browsers
  - Con: 5 MB quota too small for vectors
  - Decision: No, require IndexedDB (graceful degradation)

## Related Documents

- `docs/IMPLEMENTATION_V0.2.0_CRUD.md` - v0.2.0 CRUD implementation (format reference)
- `docs/sdk-reference/IMPLEMENTATION_WASM_BINDINGS.md` - Original WASM plan (Option B)
- `bindings/node/src/session.rs` - Node.js bindings (proven API reference)
- `docs/sdk-reference/SDK_API.md` - SDK integration guide
- `docs/API.md` - REST API documentation
- `CLAUDE.md` - Project overview and architecture

---

**Document Version**: 1.1.0
**Created**: 2025-11-04
**Updated**: 2025-11-04
**Status**: ⏳ Ready to Start Phase 1
**Scope**: Option A - Core library wrapper with tokio compatibility layer + SDK developer handoff
**Estimated Effort**: 19-21 hours (~3 days)
