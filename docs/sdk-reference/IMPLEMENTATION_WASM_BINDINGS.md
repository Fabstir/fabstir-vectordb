# WASM Bindings Implementation Plan for fabstir-ai-vector-db

## Overview

This implementation plan adds **WebAssembly (WASM) bindings** to the `fabstir-ai-vector-db` repository, enabling **browser-based vector operations** for TypeScript/JavaScript developers. This wraps existing Rust vector search code with `wasm-bindgen`, integrates with S5.js for decentralized storage, and uses IndexedDB for local caching, following strict TDD (Test-Driven Development) with bounded autonomy.

## Core Requirements

- **Package**: `@fabstir/vector-db-wasm` - WASM bindings for browsers
- **Wrapping**: Expose existing Rust vector search code to JavaScript via `#[wasm_bindgen]`
- **Storage**:
  - Primary: S5.js (decentralized, permanent)
  - Cache: IndexedDB (local disk, avoid re-downloads on app restart)
  - Runtime: WASM memory (fast search, cleared when tab closes)
- **Dimensions**: 384-dimensional vectors (matching host embeddings)
- **Performance**: <100ms search for 10K vectors (WASM overhead acceptable)
- **Browser Support**: Chrome, Firefox, Safari, Edge (latest 2 versions)
- **Bundle Size**: <500KB (gzipped WASM + JS glue code)
- **TypeScript**: Full type definitions (.d.ts)

## Architecture Integration

### Client-Side Storage Flow

```
User's Browser (Client-Side Only)
┌──────────────────────────────────────────────────────┐
│                                                      │
│  User uploads PDF → chunks → embed via host         │
│       ↓                                              │
│  ┌─────────────────────────────────────────────┐   │
│  │ WASM Vector DB (In-Memory, Fast Search)     │   │
│  │ - Rust code compiled to WASM                │   │
│  │ - Cosine similarity search                  │   │
│  │ - Cleared when tab closes ❌                │   │
│  └─────────────────────────────────────────────┘   │
│       ↓ save                    ↑ load              │
│  ┌─────────────────────────────────────────────┐   │
│  │ IndexedDB (Local Disk Cache)                │   │
│  │ - Persists across app restarts              │   │
│  │ - Avoids re-downloading from S5             │   │
│  │ - Works offline ✅                           │   │
│  └─────────────────────────────────────────────┘   │
│       ↓ backup                  ↑ restore           │
│  ┌─────────────────────────────────────────────┐   │
│  │ S5 (Decentralized Storage, Permanent)       │   │
│  │ - User's vectors stored permanently         │   │
│  │ - Accessible from any device                │   │
│  │ - Called via S5.js library ✅               │   │
│  └─────────────────────────────────────────────┘   │
│                                                      │
└──────────────────────────────────────────────────────┘
         ↕ (only for embeddings & inference)
┌──────────────────────────────────────────────────────┐
│ Host (Stateless, Multi-User, Randomly Assigned)     │
│  - POST /v1/embed (generate embeddings)             │
│  - POST /v1/inference (generate text)               │
│  - No storage, clears memory after each session     │
└──────────────────────────────────────────────────────┘
```

### Target Repository Structure

```
fabstir-ai-vector-db/
├── src/                       # Rust core (EXISTING ✅)
│   ├── lib.rs                 # Core vector search logic
│   ├── vector_store.rs        # Vector storage
│   ├── search.rs              # Cosine similarity, filtering
│   └── storage.rs             # Storage trait
├── bindings/
│   ├── node/                  # Node.js bindings (EXISTING ✅)
│   │   ├── src/lib.rs         # napi-rs bindings
│   │   └── index.js
│   └── wasm/                  # WASM bindings (NEW ← THIS)
│       ├── Cargo.toml         # wasm-bindgen deps
│       ├── src/
│       │   ├── lib.rs         # #[wasm_bindgen] entry
│       │   ├── api.rs         # VectorRAGManager wrapper
│       │   ├── storage_s5.rs  # S5.js integration
│       │   ├── storage_idb.rs # IndexedDB cache
│       │   └── types.rs       # JsValue conversions
│       ├── tests/
│       │   └── wasm.rs        # wasm-bindgen-test
│       └── pkg/               # wasm-pack output
├── package.json               # "browser" field
└── Cargo.toml                 # Workspace config
```

### Key Design Decisions

1. **Wrapping, Not Rewriting**
   - Existing Rust vector search logic stays unchanged
   - Just add `#[wasm_bindgen]` attributes
   - Estimated: ~16-20 hours (not weeks!)

2. **Storage Strategy**
   - **S5.js**: Primary storage (already integrated via JavaScript)
   - **IndexedDB**: Local cache (avoid S5 re-downloads)
   - **WASM memory**: Runtime search (fast, temporary)

3. **Browser-Only Package**
   - Node.js uses native bindings (`@fabstir/vector-db-native`)
   - Browsers use WASM bindings (`@fabstir/vector-db-wasm`)
   - Same API surface, different runtime

## Implementation Status

| Phase | Status | Tests | Estimated | Notes |
|-------|--------|-------|-----------|-------|
| Phase 1: WASM Setup & Dependencies | ⏳ Not Started | 0/8 | 2h | Add wasm-bindgen |
| Phase 2: Expose Rust API to JavaScript | ⏳ Not Started | 0/15 | 4h | #[wasm_bindgen] |
| Phase 3: S5.js Integration | ⏳ Not Started | 0/12 | 3h | Persist to S5 |
| Phase 4: IndexedDB Local Cache | ⏳ Not Started | 0/10 | 3h | Avoid re-downloads |
| Phase 5: Build System & Package | ⏳ Not Started | 0/8 | 3h | wasm-pack, npm |
| Phase 6: SDK Integration & Testing | ⏳ Not Started | 0/10 | 3h | HostAdapter, E2E |
| **TOTAL** | **0% Complete** | **0/63 tests** | **~18 hours** | **~2-3 days** |

---

## Phase 1: WASM Setup & Dependencies

### Sub-phase 1.1: Locate Repository & Analyze Structure ⏳
**Goal**: Find `fabstir-ai-vector-db` repository and understand existing code

**Tasks**:
- [ ] Locate repository (check `~/dev/Fabstir/fabstir-vectordb/` or `~/dev/Fabstir/fabstir-ai-vector-db/`)
- [ ] Verify repository structure (src/, bindings/node/, Cargo.toml)
- [ ] Analyze Node.js bindings API in `bindings/node/src/lib.rs`
- [ ] Document VectorRAGManager API surface (add, search, delete, etc.)
- [ ] Check existing storage implementation
- [ ] Review package.json current exports
- [ ] Document version (v0.2.x or v0.3.x)
- [ ] Note any existing WASM code or TODOs

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/repository_analysis_test.rs` - 8 tests
  - test_repository_located()
  - test_src_directory_exists()
  - test_node_bindings_exist()
  - test_vector_manager_api_documented()
  - test_cargo_workspace_present()
  - test_package_json_valid()
  - test_no_wasm_code_yet() (should pass initially)
  - test_s5_integration_exists()

**Success Criteria**:
- [ ] Repository found and accessible
- [ ] API surface documented (functions to wrap)
- [ ] Ready to add WASM bindings
- [ ] 8 passing analysis tests

**Deliverables**:
- [ ] Repository location confirmed
- [ ] API surface document (add, search, delete, count, etc.)
- [ ] 8 passing tests
- [ ] Notes on existing code structure

**Estimated Time**: 1 hour

**Notes**:
- Repository may be at different paths, check both locations
- Focus on what needs to be exposed to JavaScript
- Node.js bindings show exactly what API we need

---

### Sub-phase 1.2: Add WASM Dependencies ⏳
**Goal**: Add wasm-bindgen and create WASM package structure

**Tasks**:
- [ ] Create `bindings/wasm/Cargo.toml` (new package)
- [ ] Set `[lib]` with `crate-type = ["cdylib", "rlib"]`
- [ ] Add `wasm-bindgen = "0.2"` dependency
- [ ] Add `wasm-bindgen-futures = "0.4"` for async
- [ ] Add `js-sys = "0.3"` for JavaScript interop
- [ ] Add `web-sys = { version = "0.3", features = ["Window", "console", "Storage"] }`
- [ ] Add `serde-wasm-bindgen = "0.6"` for JsValue conversions
- [ ] Add `console_error_panic_hook = "0.1"` for debugging
- [ ] Add `getrandom = { version = "0.2", features = ["js"] }` for WASM random
- [ ] Update workspace `Cargo.toml`: add `members = ["bindings/wasm"]`
- [ ] Add `wasm-bindgen-test = "0.3"` (dev dependency)
- [ ] Run `cargo check -p vector-db-wasm --target wasm32-unknown-unknown`

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/dependencies_test.rs` - 10 tests
  - test_wasm_bindgen_available()
  - test_wasm_bindgen_futures_available()
  - test_js_sys_available()
  - test_web_sys_window_available()
  - test_serde_wasm_bindgen_available()
  - test_console_error_panic_hook_available()
  - test_getrandom_js_feature_enabled()
  - test_wasm_bindgen_test_available()
  - test_cargo_check_wasm32_passes()
  - test_no_conflicts_with_node_bindings()

**Success Criteria**:
- [ ] All dependencies compile for wasm32-unknown-unknown
- [ ] No conflicts with Node.js bindings
- [ ] Workspace recognizes new member
- [ ] 10 passing dependency tests

**Deliverables**:
- [ ] `bindings/wasm/Cargo.toml` (complete)
- [ ] Updated workspace `Cargo.toml`
- [ ] 10 passing tests
- [ ] Clean `cargo check` for WASM target

**Estimated Time**: 1 hour

**Notes**:
- crate-type = ["cdylib", "rlib"] required for WASM
- web-sys features: Window, console, Storage (for IndexedDB access later)
- getrandom "js" feature required for random number generation in WASM

---

## Phase 2: Expose Rust API to JavaScript

### Sub-phase 2.1: Create WASM Entry Point ⏳
**Goal**: Set up basic WASM module structure with wasm_bindgen

**Tasks**:
- [ ] Create `bindings/wasm/src/lib.rs` with `#![cfg(target_arch = "wasm32")]`
- [ ] Add `use wasm_bindgen::prelude::*;`
- [ ] Add `console_error_panic_hook::set_once()` for debugging
- [ ] Create module declarations: `mod api; mod types; mod storage_s5; mod storage_idb;`
- [ ] Add `#[wasm_bindgen(start)]` initialization function
- [ ] Create `bindings/wasm/src/api.rs` (stub for VectorRAGManager)
- [ ] Create `bindings/wasm/src/types.rs` (JsValue conversions)
- [ ] Create `bindings/wasm/src/storage_s5.rs` (S5.js integration stub)
- [ ] Create `bindings/wasm/src/storage_idb.rs` (IndexedDB stub)
- [ ] Add basic logging with `web_sys::console::log_1()`
- [ ] Verify compilation with `wasm-pack build --dev`

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/entry_point_test.rs` - 8 tests
  - test_wasm_module_compiles()
  - test_initialization_function_present()
  - test_panic_hook_configured()
  - test_console_logging_works()
  - test_api_module_exists()
  - test_types_module_exists()
  - test_storage_modules_exist()
  - test_wasm_pack_build_succeeds()

**Success Criteria**:
- [ ] WASM compiles successfully
- [ ] Modules structured correctly
- [ ] Console logging works
- [ ] 8 passing entry point tests

**Deliverables**:
- [ ] `bindings/wasm/src/lib.rs`
- [ ] `bindings/wasm/src/api.rs` (stub)
- [ ] `bindings/wasm/src/types.rs` (stub)
- [ ] `bindings/wasm/src/storage_s5.rs` (stub)
- [ ] `bindings/wasm/src/storage_idb.rs` (stub)
- [ ] 8 passing tests

**Estimated Time**: 1.5 hours

---

### Sub-phase 2.2: Wrap VectorRAGManager ⏳
**Goal**: Expose existing Rust VectorRAGManager to JavaScript

**Tasks**:
- [ ] In `bindings/wasm/src/api.rs`, create `#[wasm_bindgen]` struct VectorRAGManager
- [ ] Add internal field: `inner: Arc<Mutex<VectorStore>>` (from core lib)
- [ ] Implement `#[wasm_bindgen] impl VectorRAGManager`
- [ ] Add constructor: `pub fn new(dimensions: usize, database_name: String) -> Result<VectorRAGManager, JsValue>`
- [ ] Validate dimensions = 384
- [ ] Add `pub async fn add(&mut self, id: String, vector: Vec<f32>, metadata: JsValue) -> Result<bool, JsValue>`
- [ ] Add `pub async fn search(&self, query: Vec<f32>, k: usize) -> Result<JsValue, JsValue>`
- [ ] Add `pub async fn delete(&mut self, id: String) -> Result<bool, JsValue>`
- [ ] Add `pub async fn count(&self) -> Result<usize, JsValue>`
- [ ] Implement JsValue error conversion
- [ ] Add proper Result handling (no panics!)

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/vector_manager_api_test.rs` - 12 tests
  - test_constructor_with_384_dimensions()
  - test_constructor_rejects_invalid_dimensions()
  - test_add_single_vector()
  - test_add_with_metadata()
  - test_search_returns_results()
  - test_delete_vector()
  - test_count_vectors()
  - test_error_handling_no_panics()
  - test_async_operations()
  - test_concurrent_operations()
  - test_jsvalue_conversions()
  - test_api_matches_node_bindings()

**Success Criteria**:
- [ ] All methods exposed to JavaScript
- [ ] No panics (only Result<T, JsValue>)
- [ ] Async operations work
- [ ] 12 passing API tests

**Deliverables**:
- [ ] Complete `bindings/wasm/src/api.rs`
- [ ] VectorRAGManager with 5 methods (new, add, search, delete, count)
- [ ] Error handling with JsValue
- [ ] 12 passing tests

**Estimated Time**: 2.5 hours

---

## Phase 3: S5.js Integration

### Sub-phase 3.1: S5 Storage Adapter ⏳
**Goal**: Integrate S5.js for permanent decentralized storage

**Tasks**:
- [ ] In `bindings/wasm/src/storage_s5.rs`, create S5Storage struct
- [ ] Add `#[wasm_bindgen(module = "/src/s5_bindings.js")]` external JS functions
- [ ] Define `extern "C" { async fn s5_upload(data: JsValue) -> Promise; }`
- [ ] Define `extern "C" { async fn s5_download(cid: String) -> Promise; }`
- [ ] Create `s5_bindings.js` wrapper around S5.js library
- [ ] Implement `async fn save_to_s5(&self, vectors: Vec<Vector>) -> Result<String, JsValue>`
- [ ] Implement `async fn load_from_s5(&self, cid: &str) -> Result<Vec<Vector>, JsValue>`
- [ ] Add chunking for large datasets (10K vectors per chunk)
- [ ] Return S5 CID after upload
- [ ] Handle network errors gracefully

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/s5_integration_test.rs` - 12 tests
  - test_s5_upload_vectors()
  - test_s5_download_vectors()
  - test_s5_cid_returned()
  - test_s5_chunking_large_dataset()
  - test_s5_network_error_handling()
  - test_s5_retry_logic()
  - test_s5_multiple_chunks()
  - test_s5_vector_integrity()
  - test_s5_js_bindings_available()
  - test_s5_async_operations()
  - test_s5_offline_graceful_degradation()
  - test_s5_concurrent_uploads()

**Success Criteria**:
- [ ] Vectors persist to S5 network
- [ ] CID returned for retrieval
- [ ] Chunking works for large datasets
- [ ] 12 passing S5 tests

**Deliverables**:
- [ ] `bindings/wasm/src/storage_s5.rs`
- [ ] `bindings/wasm/src/s5_bindings.js`
- [ ] Save/load functionality
- [ ] 12 passing tests

**Estimated Time**: 3 hours

**Notes**:
- S5.js already handles encryption
- Use js-sys::Promise for async JavaScript calls
- Chunk size: 10K vectors (adjust based on S5 limits)

---

## Phase 4: IndexedDB Local Cache

### Sub-phase 4.1: IndexedDB Storage Adapter ⏳
**Goal**: Implement IndexedDB cache to avoid S5 re-downloads

**Tasks**:
- [ ] In `bindings/wasm/src/storage_idb.rs`, create IDBStorage struct
- [ ] Add web-sys features: `["IdbDatabase", "IdbObjectStore", "IdbTransaction", "IdbRequest"]`
- [ ] Implement `async fn init_db(&self) -> Result<(), JsValue>` (create object stores)
- [ ] Implement `async fn save_to_idb(&self, vectors: Vec<Vector>) -> Result<(), JsValue>`
- [ ] Implement `async fn load_from_idb(&self) -> Result<Vec<Vector>, JsValue>`
- [ ] Implement `async fn clear_idb(&self) -> Result<(), JsValue>`
- [ ] Handle IDB transactions and promises
- [ ] Add error handling for quota exceeded
- [ ] Implement versioning for schema upgrades

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/indexeddb_test.rs` - 10 tests
  - test_idb_initialization()
  - test_idb_save_vectors()
  - test_idb_load_vectors()
  - test_idb_clear_database()
  - test_idb_quota_exceeded_handling()
  - test_idb_transaction_commit()
  - test_idb_persists_across_reload()
  - test_idb_version_upgrade()
  - test_idb_concurrent_operations()
  - test_idb_fallback_when_unavailable()

**Success Criteria**:
- [ ] IndexedDB stores vectors locally
- [ ] Data persists across tab closes
- [ ] Quota errors handled
- [ ] 10 passing IDB tests

**Deliverables**:
- [ ] `bindings/wasm/src/storage_idb.rs`
- [ ] Database initialization
- [ ] Save/load/clear operations
- [ ] 10 passing tests

**Estimated Time**: 3 hours

**Notes**:
- Test in real browser (wasm-pack test --headless --chrome)
- IndexedDB is async, use wasm-bindgen-futures
- Handle private browsing mode (IDB unavailable)

---

## Phase 5: Build System & Package

### Sub-phase 5.1: Configure wasm-pack Build ⏳
**Goal**: Set up wasm-pack for production builds

**Tasks**:
- [ ] Create `bindings/wasm/Cargo.toml` with `[package.metadata.wasm-pack.profile.release]`
- [ ] Enable wasm-opt: `wasm-opt = ["-Oz", "--enable-simd"]`
- [ ] Configure target: `"web"` (ES modules)
- [ ] Set up `.cargo/config.toml` with WASM optimizations
- [ ] Add build script: `wasm-pack build --target web --out-dir pkg`
- [ ] Configure source maps for debugging
- [ ] Test build: `wasm-pack build --target web`
- [ ] Verify output in `pkg/` directory
- [ ] Check bundle size (<500KB uncompressed)
- [ ] Test gzipped size (<200KB)

**Test Files** (TDD - Written First):
- `bindings/wasm/tests/build_test.sh` - 8 tests
  - test_wasm_pack_build_succeeds()
  - test_pkg_directory_created()
  - test_wasm_file_size_acceptable() (<500KB)
  - test_gzipped_size_acceptable() (<200KB)
  - test_js_glue_code_generated()
  - test_typescript_definitions_generated()
  - test_package_json_in_pkg()
  - test_wasm_opt_applied()

**Success Criteria**:
- [ ] Build succeeds
- [ ] Bundle size <500KB
- [ ] TypeScript defs generated
- [ ] 8 passing build tests

**Deliverables**:
- [ ] Configured wasm-pack build
- [ ] Build script
- [ ] Optimized WASM output
- [ ] 8 passing tests

**Estimated Time**: 1.5 hours

---

### Sub-phase 5.2: NPM Package Configuration ⏳
**Goal**: Create package.json with dual exports (Node.js + browser)

**Tasks**:
- [ ] Create root `package.json` in fabstir-ai-vector-db
- [ ] Set `"name": "@fabstir/vector-db"`
- [ ] Configure `"main": "./bindings/node/index.js"` (Node.js)
- [ ] Configure `"browser": "./bindings/wasm/pkg/vector_db_wasm.js"` (browser)
- [ ] Add `"exports"` field with conditional exports
- [ ] Add `"type": "module"`
- [ ] Add `"types": "./bindings/wasm/pkg/vector_db_wasm.d.ts"`
- [ ] Add scripts: `"build": "wasm-pack build ..."`
- [ ] Add keywords, description, license
- [ ] Test `npm pack` (check tarball contents)

**Deliverables**:
- [ ] package.json with dual exports
- [ ] npm scripts
- [ ] Metadata complete

**Estimated Time**: 1.5 hours

---

## Phase 6: SDK Integration & Testing

### Sub-phase 6.1: Update HostAdapter for WASM ⏳
**Goal**: Integrate WASM bindings into SDK's HostAdapter

**Tasks**:
- [ ] Update SDK's HostAdapter to import `@fabstir/vector-db`
- [ ] Add browser detection: `typeof window !== 'undefined'`
- [ ] Initialize VectorRAGManager in browser
- [ ] Connect to /v1/embed endpoint for embeddings
- [ ] Store embeddings in WASM vector DB
- [ ] Implement search using WASM
- [ ] Add IndexedDB caching
- [ ] Add S5 persistence
- [ ] Test end-to-end workflow

**Test Files** (TDD - Written First):
- `packages/sdk-core/tests/host_adapter_wasm_test.ts` - 10 tests
  - test_wasm_initialization()
  - test_embed_via_host()
  - test_store_in_wasm_db()
  - test_search_wasm_db()
  - test_idb_cache_hit()
  - test_s5_persistence()
  - test_offline_mode()
  - test_full_rag_workflow()
  - test_next_js_integration()
  - test_browser_compatibility()

**Success Criteria**:
- [ ] HostAdapter works in browser
- [ ] Full RAG workflow functional
- [ ] Offline mode works
- [ ] 10 passing integration tests

**Deliverables**:
- [ ] Updated HostAdapter
- [ ] WASM integration complete
- [ ] 10 passing tests
- [ ] Example Next.js app

**Estimated Time**: 3 hours

---

## Development Guidelines

### Do's ✅
- Write tests BEFORE implementation (strict TDD)
- Use `Result<T, JsValue>` for all error handling
- Add `#[wasm_bindgen]` attributes to public APIs
- Log errors to console (not panics!)
- Test in real browsers (not just Node.js)
- Document TypeScript types
- Handle offline mode gracefully

### Don'ts ❌
- Never skip TDD (tests first, always!)
- Never panic in WASM (browsers can't recover)
- Never log user vectors/metadata (privacy!)
- Never expose raw pointers to JavaScript
- Never block browser main thread
- Never skip bundle size optimization
- Never assume network is always available

---

## Timeline Estimate

| Phase | Time |
|-------|------|
| Phase 1: WASM Setup & Dependencies | 2h |
| Phase 2: Expose Rust API | 4h |
| Phase 3: S5.js Integration | 3h |
| Phase 4: IndexedDB Cache | 3h |
| Phase 5: Build & Package | 3h |
| Phase 6: SDK Integration | 3h |
| **TOTAL** | **~18 hours (~2-3 days)** |

---

## Success Metrics

### Test Coverage
- **Target**: 63 total tests
- **Current**: 0/63 (0%)
- **Minimum**: 90% pass rate

### Performance
- **Bundle Size**: <500KB WASM + JS (gzipped <200KB)
- **Search**: <100ms for 10K vectors
- **Startup**: <2s (with IndexedDB cache)

### Browser Support
- Chrome, Firefox, Safari, Edge (latest 2) ✅
- Mobile: Chrome Android, Safari iOS ✅

---

## Next Steps

1. ✅ Document created with correct architecture
2. ⏭️ Start Phase 1: Locate fabstir-ai-vector-db repository
3. ⏭️ Follow strict TDD (tests first!)
4. ⏭️ Mark progress with [x]

---

**Document Version**: 2.0.0
**Created**: November 4, 2025
**Updated**: November 4, 2025 (corrected architecture)
**Status**: ⏳ Ready to Start Phase 1
**Scope**: Wrapping existing Rust code, not rewriting (realistic ~18 hours)
