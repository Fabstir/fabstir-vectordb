# Chunked Vector Storage with Lazy Loading - Implementation Plan

## Project Overview

Implement chunked vector storage with lazy loading to support large-scale vector databases (1M+ vectors) with efficient memory usage and fast startup times. This replaces the monolithic CBOR blob approach with a manifest-based chunked architecture.

**Note:** No migration from old format needed - MVP does not use RAG yet. This is a clean breaking change.

## Architecture Summary

### Storage Layout (S5)
```
session-123/
├── manifest.json                    # Unencrypted, fast load (~1-10 MB)
│   ├── version, chunk_size, total_vectors
│   ├── chunks: [{id, cid, vector_count, range}]
│   ├── hnsw_graph (connectivity only, no vectors)
│   └── ivf_centroids + cluster→chunk mapping
├── chunks/
│   ├── chunk-0.cbor                # Encrypted by default (10K vectors, ~15 MB)
│   ├── chunk-1.cbor
│   └── ...
└── metadata.cbor                   # Encrypted (user metadata map)
```

### Key Design Decisions
- **Chunk Size**: 10K vectors/chunk (~15 MB for 384D embeddings)
- **Encryption**: ON by default (XChaCha20-Poly1305 via S5.js)
- **Caching**: LRU cache with size limit (memory-bounded)
- **Compatibility**: Breaking change, no backward compatibility
- **Manifest**: Unencrypted for fast startup (<1 sec)

## Current Status

- ✅ Phase 1: Core Chunking Infrastructure (100%) - Completed 2025-01-XX
- ⏳ Phase 2: Chunked Persistence Layer (0%)
- ⏳ Phase 3: Enhanced S5 with Encryption (0%)
- ⏳ Phase 4: HNSW/IVF Lazy Loading (0%)
- ⏳ Phase 5: Node.js Bindings Updates (0%)
- ⏳ Phase 6: Integration Testing & Benchmarks (0%)
- ⏳ Phase 7: Documentation Updates (0%)

## Implementation Phases

### Phase 1: Core Chunking Infrastructure (3-4 days)

Foundation types for chunking, manifest, and LRU cache.

#### 1.1 Chunk Types & Manifest (Day 1 - Morning) ✅ 2025-01-XX

**TDD Approach**: Write tests first, then implement

- [x] **Test File**: `tests/unit/chunk_tests.rs` (created, 17 tests, all passing)
  - [x] Test `VectorChunk` creation and serialization
  - [x] Test `ChunkMetadata` CBOR roundtrip
  - [x] Test `Manifest` JSON serialization/deserialization
  - [x] Test manifest validation (version checking)
  - [x] Test chunk range validation (no overlaps)
  - [x] Property tests for chunk partitioning

- [x] **Implementation**: `src/core/chunk.rs` (created, ~390 lines)
  - [x] Define `VectorChunk` struct
    - `chunk_id: String`
    - `vectors: HashMap<VectorId, Vec<f32>>`
    - `start_idx: usize, end_idx: usize`
  - [x] Define `ChunkMetadata` struct
    - `chunk_id: String`
    - `cid: Option<String>` (S5 CID after upload)
    - `vector_count: usize`
    - `byte_size: usize`
    - `vector_id_range: (VectorId, VectorId)`
  - [x] Define `Manifest` struct
    - `version: u32` (current: 2)
    - `chunk_size: usize` (default: 10000)
    - `total_vectors: usize`
    - `chunks: Vec<ChunkMetadata>`
    - `hnsw_structure: Option<HNSWManifest>`
    - `ivf_structure: Option<IVFManifest>`
  - [x] Define `HNSWManifest` (graph without vectors)
    - `entry_point: VectorId`
    - `layers: Vec<LayerMetadata>`
    - `node_chunk_map: HashMap<String, String>` (node_id → chunk_id)
  - [x] Define `IVFManifest`
    - `centroids: Vec<Vec<f32>>` (keep in memory, ~256 × 384)
    - `cluster_assignments: HashMap<usize, Vec<String>>` (cluster → chunk_ids)
  - [x] Implement CBOR serialization for chunks
  - [x] Implement JSON serialization for manifest
  - [x] Add validation methods

**Bounded Autonomy**: ✅ 390 lines (within 400 line limit)

**Test Results**: ✅ All 17 tests passing

#### 1.2 LRU Chunk Cache (Day 1 - Afternoon) ✅ 2025-01-XX

**TDD Approach**: Write tests first

- [x] **Test File**: `tests/unit/chunk_cache_tests.rs` (created, 19 tests, all passing)
  - [x] Test cache insertion and retrieval
  - [x] Test LRU eviction policy (access order)
  - [x] Test size-based eviction (memory limit)
  - [x] Test concurrent access (thread safety)
  - [x] Test cache hit/miss metrics
  - [x] Test cache clear operation

- [x] **Implementation**: `src/core/chunk_cache.rs` (created, ~280 lines)
  - [x] Define `ChunkCache` struct
    - `cache: Arc<RwLock<LruCache<String, VectorChunk>>>`
    - `capacity: usize` (configurable)
    - `metrics: Arc<RwLock<CacheMetrics>>`
  - [x] Define `CacheMetrics`
    - `hits: u64, misses: u64, evictions: u64`
    - `total_requests()`, `hit_rate()`, `reset()` methods
  - [x] Implement `get(&self, chunk_id: &str) -> Option<VectorChunk>`
  - [x] Implement `put(&self, chunk_id: String, chunk: VectorChunk)`
  - [x] Implement `contains(&self, chunk_id: &str) -> bool`
  - [x] Implement `clear(&self)`
  - [x] Implement `get_metrics(&self) -> CacheMetrics`
  - [x] Thread-safe implementation with `RwLock`
  - [x] Implement `Clone` and `Debug` traits

**Dependencies**: ✅ Added `lru = "0.12"` to Cargo.toml

**Bounded Autonomy**: ✅ 280 lines (within 300 line limit)

**Test Results**: ✅ All 19 tests passing (including concurrent access tests)

#### 1.3 Update Core Module (Day 1 - Evening) ✅ 2025-01-XX

- [x] **Modify**: `src/core/mod.rs`
  - [x] Add `pub mod chunk;`
  - [x] Add `pub mod chunk_cache;`
  - [x] Export public types (`ChunkCache`, `CacheMetrics`, etc.)

- [x] **Modify**: `Cargo.toml`
  - [x] Add `lru = "0.12"` dependency

- [x] **Run Tests**
  - [x] `cargo test --lib core::chunk` - ✅ 3 tests passing
  - [x] `cargo test --test unit_tests` - ✅ 36 tests passing (17 chunk + 19 cache)
  - [x] All tests pass

**Notes**: Phase 1 (Core Chunking Infrastructure) is now complete!

---

### Phase 2: Chunked Persistence Layer (4-5 days)

Rewrite `HybridPersister` to use chunked storage with manifest.

#### 2.1 Chunked Save Operations (Day 2 - Full Day)

**TDD Approach**: Write integration tests first

- [ ] **Test File**: `tests/integration/chunked_save_tests.rs` (create new, max 400 lines)
  - [ ] Test save empty index (manifest only)
  - [ ] Test save index with <10K vectors (1 chunk)
  - [ ] Test save index with 25K vectors (3 chunks)
  - [ ] Test save index with 100K vectors (10 chunks)
  - [ ] Test chunk metadata accuracy (counts, ranges)
  - [ ] Test manifest generation (chunk list, index structures)
  - [ ] Test HNSW structure preservation in manifest
  - [ ] Test IVF structure preservation in manifest
  - [ ] Verify S5 storage calls (mock backend)
  - [ ] Test encryption flag respected

- [ ] **Implementation**: `src/hybrid/persistence.rs` (major rewrite, target 600 lines)
  - [ ] Add `save_index_chunked()` method to `HybridPersister`
  - [ ] Partition vectors into chunks (by VectorId order)
    - Algorithm: Iterate through all vectors, batch into 10K chunks
  - [ ] Serialize each chunk as CBOR
  - [ ] Upload chunks to S5 with encryption
    - Path: `{base_path}/chunks/chunk-{i}.cbor`
  - [ ] Build `HNSWManifest` (extract connectivity graph)
    - Store node → chunk_id mapping
  - [ ] Build `IVFManifest` (extract centroids + cluster assignments)
    - Store cluster → chunk_ids mapping
  - [ ] Generate `Manifest` with all metadata
  - [ ] Save manifest as JSON (unencrypted)
    - Path: `{base_path}/manifest.json`
  - [ ] Save user metadata map (encrypted)
    - Path: `{base_path}/metadata.cbor`
  - [ ] Return manifest with CIDs

**Bounded Autonomy**: Target 600 lines for persistence.rs. If exceeding, extract helper modules.

**Notes**:

#### 2.2 Chunked Load Operations (Day 3 - Full Day)

**TDD Approach**: Write integration tests first

- [ ] **Test File**: `tests/integration/chunked_load_tests.rs` (create new, max 400 lines)
  - [ ] Test load manifest (fast, no chunks loaded yet)
  - [ ] Test lazy load single chunk
  - [ ] Test lazy load multiple chunks in parallel
  - [ ] Test load with missing chunks (error handling)
  - [ ] Test load with corrupted manifest (error handling)
  - [ ] Test load with version mismatch (error handling)
  - [ ] Test HNSW structure reconstruction
  - [ ] Test IVF structure reconstruction
  - [ ] Test vector count preservation
  - [ ] Test search after load (correctness)

- [ ] **Implementation**: `src/hybrid/persistence.rs` (continue from 2.1)
  - [ ] Add `load_index_chunked()` method to `HybridPersister`
  - [ ] Load manifest.json from S5 (fast, unencrypted)
  - [ ] Parse manifest and validate version
  - [ ] Reconstruct HNSW graph structure (no vectors yet)
  - [ ] Reconstruct IVF centroids + cluster metadata
  - [ ] Initialize chunk cache
  - [ ] Create `ChunkLoader` for lazy loading
  - [ ] Load user metadata map (encrypted)
  - [ ] Return partially-loaded `HybridIndex` (vectors load on-demand)

**Bounded Autonomy**: Keep within 600 lines total for persistence.rs.

**Notes**:

#### 2.3 Manifest Upgrade Path (Day 3 - Evening)

- [ ] **Test File**: `tests/integration/manifest_version_tests.rs` (create new, max 150 lines)
  - [ ] Test version 2 manifest parsing
  - [ ] Test unsupported version rejection
  - [ ] Test missing required fields

- [ ] **Implementation**: `src/hybrid/persistence.rs`
  - [ ] Add `CURRENT_MANIFEST_VERSION = 2` constant
  - [ ] Add version validation in `load_index_chunked()`
  - [ ] Return `PersistenceError::IncompatibleVersion` for version > 2

**Notes**:

---

### Phase 3: Enhanced S5 with Encryption (2-3 days)

Add encryption support and chunk loader.

#### 3.1 Encryption Configuration (Day 4 - Morning)

**TDD Approach**: Write tests first

- [ ] **Test File**: `tests/integration/s5_encryption_tests.rs` (create new, max 200 lines)
  - [ ] Test encryption ON by default
  - [ ] Test explicit encryption disable
  - [ ] Test encryption headers sent to S5.js
  - [ ] Test decryption on get (handled by S5.js)
  - [ ] Test encryption with mock S5 backend

- [ ] **Implementation**: `src/storage/enhanced_s5_storage.rs` (modify, add ~100 lines)
  - [ ] Add `encrypt_at_rest: bool` to `S5StorageConfig` (default: true)
  - [ ] Modify `put_raw()` to include encryption headers
    - If `encrypt_at_rest == true`, add header:
      ```json
      {
        "X-S5-Encryption": "xchacha20-poly1305"
      }
      ```
  - [ ] Ensure `get_raw()` handles decryption transparently
  - [ ] Update `get_stats()` to show encryption status

**Bounded Autonomy**: Add ~100 lines max to enhanced_s5_storage.rs.

**Notes**:

#### 3.2 Chunk Loader (Day 4 - Afternoon + Day 5 Morning)

**TDD Approach**: Write tests first

- [ ] **Test File**: `tests/integration/chunk_loader_tests.rs` (create new, max 300 lines)
  - [ ] Test load single chunk
  - [ ] Test load multiple chunks in parallel
  - [ ] Test chunk not found (404 error)
  - [ ] Test S5 timeout handling
  - [ ] Test retry logic with exponential backoff
  - [ ] Test cache integration
  - [ ] Test decryption (via S5.js)
  - [ ] Test concurrent load requests (deduplication)

- [ ] **Implementation**: `src/storage/chunk_loader.rs` (create new, max 300 lines)
  - [ ] Define `ChunkLoader` struct
    - `storage: Arc<dyn S5Storage>`
    - `cache: Arc<ChunkCache>`
    - `in_flight: Arc<RwLock<HashMap<String, JoinHandle>>>` (deduplication)
  - [ ] Implement `load_chunk(&self, path: &str) -> Result<VectorChunk>`
    - Check cache first
    - If not cached, load from S5 (with retries)
    - Deserialize CBOR
    - Store in cache
    - Return chunk
  - [ ] Implement `load_chunks_parallel(&self, paths: Vec<String>) -> Result<Vec<VectorChunk>>`
    - Use `tokio::spawn` for parallel loading
    - Deduplicate in-flight requests
    - Return all chunks
  - [ ] Implement retry logic with exponential backoff
    - Max retries: 3
    - Backoff: 100ms, 200ms, 400ms

**Bounded Autonomy**: Max 300 lines.

#### 3.3 Update Storage Module (Day 5 - Afternoon)

- [ ] **Modify**: `src/storage/mod.rs`
  - [ ] Add `pub mod chunk_loader;`
  - [ ] Export `ChunkLoader`

- [ ] **Run Tests**
  - [ ] `cargo test --lib storage::chunk_loader`
  - [ ] `cargo test --integration s5_encryption_tests`
  - [ ] All tests should pass

**Notes**:

---

### Phase 4: HNSW/IVF Lazy Loading (5-6 days)

Adapt HNSW and IVF indices to support lazy vector loading.

#### 4.1 HNSW Lazy Loading (Day 6-7)

**TDD Approach**: Write integration tests first

- [ ] **Test File**: `tests/integration/hnsw_lazy_tests.rs` (create new, max 400 lines)
  - [ ] Test HNSW search with lazy vector loading
  - [ ] Test HNSW search across multiple chunks
  - [ ] Test cache hit rate during repeated searches
  - [ ] Test HNSW insert with lazy-loaded neighbors
  - [ ] Test performance: cold cache vs. warm cache
  - [ ] Test error handling: missing chunk
  - [ ] Test concurrent searches (thread safety)

- [ ] **Implementation**: `src/hnsw/core.rs` (major refactor, target ~800 lines)
  - [ ] Add `chunk_loader: Option<Arc<ChunkLoader>>` to `HNSWIndex`
  - [ ] Separate vector storage from graph structure
    - Current: Nodes store `vector: Vec<f32>` inline
    - New: Nodes store `vector_chunk_id: Option<String>`
  - [ ] Add `get_vector(&self, id: &VectorId) -> Result<Vec<f32>>`
    - If vector in memory, return immediately
    - Else, load chunk via `chunk_loader`
    - Cache vector in node
  - [ ] Modify `search()` to use `get_vector()` for distance calculations
  - [ ] Modify `insert()` to track chunk assignments
  - [ ] Add `preload_chunks(&self, chunk_ids: Vec<String>)` for batch loading
  - [ ] Update `HNSWConfig` to include chunk_loader option

**Bounded Autonomy**: Target ~800 lines. If exceeding, split lazy loading into separate module.

**Notes**:

#### 4.2 IVF Lazy Loading (Day 8-9)

**TDD Approach**: Write integration tests first

- [ ] **Test File**: `tests/integration/ivf_lazy_tests.rs` (create new, max 400 lines)
  - [ ] Test IVF search with lazy cluster loading
  - [ ] Test multi-probe search across chunks
  - [ ] Test cache hit rate for hot clusters
  - [ ] Test IVF insert to lazy-loaded cluster
  - [ ] Test performance: cold cache vs. warm cache
  - [ ] Test error handling: missing chunk
  - [ ] Test cluster rebalancing with lazy loading

- [ ] **Implementation**: `src/ivf/core.rs` (major refactor, target ~900 lines)
  - [ ] Add `chunk_loader: Option<Arc<ChunkLoader>>` to `IVFIndex`
  - [ ] Keep centroids in memory (only 256 × 384 = ~400 KB)
  - [ ] Modify inverted lists to store chunk references
    - Current: Inverted lists store full vectors
    - New: `InvertedList` stores `Vec<(VectorId, ChunkId)>`
  - [ ] Add `get_cluster_vectors(&self, cluster_id: ClusterId) -> Result<Vec<(VectorId, Vec<f32>)>>`
    - Determine which chunks contain cluster vectors
    - Load chunks via `chunk_loader`
    - Return vectors for the cluster
  - [ ] Modify `search()` to use `get_cluster_vectors()`
  - [ ] Modify `insert()` to track chunk assignments
  - [ ] Update `IVFConfig` to include chunk_loader option

**Bounded Autonomy**: Target ~900 lines. If exceeding, split inverted list management into separate file.

**Notes**:

#### 4.3 HybridIndex Integration (Day 10)

**TDD Approach**: Write integration tests

- [ ] **Test File**: `tests/integration/hybrid_lazy_tests.rs` (create new, max 500 lines)
  - [ ] Test hybrid search with lazy loading (both HNSW + IVF)
  - [ ] Test chunk cache shared between HNSW and IVF
  - [ ] Test search correctness: compare to eager-loaded baseline
  - [ ] Test large dataset: 100K vectors (10 chunks)
  - [ ] Test memory usage: verify <200 MB with 3 chunks cached
  - [ ] Test cold start: measure first search latency
  - [ ] Test warm cache: measure subsequent search latency

- [ ] **Implementation**: `src/hybrid/core.rs` (modify, add ~200 lines)
  - [ ] Add `chunk_loader: Arc<ChunkLoader>` to `HybridIndex`
  - [ ] Pass chunk_loader to HNSW and IVF indices on creation
  - [ ] Update `from_parts()` to accept chunk_loader
  - [ ] Update `initialize()` to set up chunk_loader
  - [ ] Ensure search uses lazy loading transparently

**Bounded Autonomy**: Add ~200 lines to hybrid/core.rs.

**Notes**:

---

### Phase 5: Node.js Bindings Updates (3-4 days)

Update Node.js bindings to support chunked loading and encryption.

#### 5.1 Update Session Config (Day 11 - Morning)

**TDD Approach**: Update TypeScript tests first

- [ ] **Test File**: `bindings/node/__test__/session_config.spec.ts` (create new, max 200 lines)
  - [ ] Test default encryption (ON)
  - [ ] Test explicit encryption disable
  - [ ] Test custom chunk_size
  - [ ] Test custom cache_size_mb
  - [ ] Test config validation (invalid values)

- [ ] **Implementation**: `bindings/node/src/types.rs` (modify, add ~50 lines)
  - [ ] Add fields to `VectorDBConfig`:
    - `encrypt_at_rest: Option<bool>` (default: true)
    - `chunk_size: Option<u32>` (default: 10000)
    - `cache_size_mb: Option<u32>` (default: 150)

**Bounded Autonomy**: Add ~50 lines to types.rs.

#### 5.2 Update Load Operation (Day 11 - Afternoon + Day 12)

**TDD Approach**: Write tests first

- [ ] **Test File**: `bindings/node/__test__/chunked_load.spec.ts` (create new, max 400 lines)
  - [ ] Test load with encryption ON
  - [ ] Test load with encryption OFF
  - [ ] Test load with progress callback
  - [ ] Test load large dataset (50K vectors)
  - [ ] Test search after chunked load
  - [ ] Test add vectors after chunked load
  - [ ] Test save after chunked load (roundtrip)
  - [ ] Test error handling: missing manifest
  - [ ] Test error handling: corrupted chunk

- [ ] **Implementation**: `bindings/node/src/session.rs` (modify, add ~150 lines)
  - [ ] Update `load_user_vectors()` to use chunked persistence
  - [ ] Add optional progress callback parameter
    - `on_progress?: (loaded: number, total: number) => void`
  - [ ] Call chunked load from `HybridPersister`
  - [ ] Emit progress events during chunk loading
  - [ ] Return `LoadStats`:
    - `total_vectors: number`
    - `chunks_loaded: number`
    - `cache_size_mb: number`
    - `load_time_ms: number`

**Bounded Autonomy**: Add ~150 lines to session.rs.

#### 5.3 Update Save Operation (Day 13)

**TDD Approach**: Write tests first

- [ ] **Test File**: `bindings/node/__test__/chunked_save.spec.ts` (create new, max 300 lines)
  - [ ] Test save with encryption ON
  - [ ] Test save with encryption OFF
  - [ ] Test save large dataset (50K vectors)
  - [ ] Test save-load roundtrip
  - [ ] Test manifest generation
  - [ ] Test chunk count accuracy

- [ ] **Implementation**: `bindings/node/src/session.rs` (modify, add ~50 lines)
  - [ ] Update `save_to_s5()` to use chunked persistence
  - [ ] Return manifest CID as the session reference
  - [ ] Add optional progress callback

**Bounded Autonomy**: Add ~50 lines to session.rs.

#### 5.4 Update TypeScript Types (Day 13 - Evening)

- [ ] **Modify**: `bindings/node/index.d.ts` (add type definitions)
  - [ ] Add `encryptAtRest?: boolean` to `VectorDBConfig`
  - [ ] Add `chunkSize?: number` to `VectorDBConfig`
  - [ ] Add `cacheSizeMb?: number` to `VectorDBConfig`
  - [ ] Add `LoadStats` interface
  - [ ] Add `onProgress?: (loaded: number, total: number) => void` to `LoadOptions`

- [ ] **Run Tests**
  - [ ] `cd bindings/node && npm test`
  - [ ] All tests should pass

**Notes**:

---

### Phase 6: Integration Testing & Benchmarks (2-3 days)

End-to-end testing with large datasets and performance validation.

#### 6.1 Large Dataset Tests (Day 14)

- [ ] **Test File**: `tests/integration/large_dataset_tests.rs` (create new, max 500 lines)
  - [ ] Test 100K vectors: save + load + search
    - Verify search correctness
    - Measure load time (target: <5 sec)
    - Measure memory usage (target: <200 MB with 3 chunks cached)
  - [ ] Test 500K vectors: save + load + search
    - Verify chunk count (50 chunks)
    - Measure load time (target: <10 sec)
    - Measure search latency (target: <100ms)
  - [ ] Test 1M vectors: save + load + search
    - Verify chunk count (100 chunks)
    - Measure load time (target: <15 sec)
    - Measure search latency (target: <150ms)

- [ ] **Run Tests**
  - [ ] `cargo test --release --test large_dataset_tests`
  - [ ] Capture performance metrics

**Notes**:

#### 6.2 Performance Benchmarks (Day 15 - Morning)

- [ ] **Benchmark File**: `benches/chunked_search_bench.rs` (create new, max 300 lines)
  - [ ] Benchmark cold cache search (first search after load)
  - [ ] Benchmark warm cache search (repeated searches)
  - [ ] Benchmark chunk loading overhead
  - [ ] Compare to monolithic format baseline
    - Monolithic load time: ~30 sec for 100K vectors
    - Chunked load time: <5 sec (6x improvement)
  - [ ] Measure cache hit rate over 1000 searches

- [ ] **Run Benchmarks**
  - [ ] `cargo bench --bench chunked_search_bench`
  - [ ] Document results in notes

**Notes**:

#### 6.3 Memory Profiling (Day 15 - Afternoon)

- [ ] **Script**: `scripts/profile_memory.sh` (create new)
  - [ ] Profile memory usage during load
  - [ ] Profile memory usage during search
  - [ ] Profile cache eviction behavior
  - [ ] Verify memory bounds: <200 MB for 10 chunks cached

- [ ] **Run Profiling**
  - [ ] `./scripts/profile_memory.sh`
  - [ ] Document results

**Notes**:

#### 6.4 Node.js E2E Tests (Day 15 - Evening)

- [ ] **Test File**: `bindings/node/__test__/e2e_chunked.spec.ts` (create new, max 400 lines)
  - [ ] Full workflow: create session → add 50K vectors → save → destroy
  - [ ] Full workflow: create session → load → search → destroy
  - [ ] Test encryption roundtrip
  - [ ] Test multiple concurrent sessions
  - [ ] Test cache limits enforced
  - [ ] Measure total memory usage

- [ ] **Run Tests**
  - [ ] `cd bindings/node && npm test`
  - [ ] All E2E tests pass

**Notes**:

---

### Phase 7: Documentation Updates (2 days)

Update all documentation for the new chunked architecture.

#### 7.1 Update CLAUDE.md (Day 16 - Morning)

- [ ] **Modify**: `/workspace/CLAUDE.md`
  - [ ] Update architecture section (chunked storage)
  - [ ] Update size limits: now supports 1M+ vectors
  - [ ] Document encryption defaults (ON by default)
  - [ ] Add chunked loading configuration
  - [ ] Add cache tuning guide
  - [ ] Update environment variables:
    - `VECTOR_DB_ENCRYPT_AT_REST=true` (default)
    - `VECTOR_DB_CHUNK_SIZE=10000`
    - `VECTOR_DB_CACHE_SIZE_MB=150`
  - [ ] Update troubleshooting section

**Notes**:

#### 7.2 Update Vector DB Integration Guide (Day 16 - Afternoon)

- [ ] **Modify**: `docs/sdk-reference/VECTOR_DB_INTEGRATION.md`
  - [ ] Update API examples with encryption config
  - [ ] Document breaking change (no migration needed for MVP)
  - [ ] Add chunked loading examples
  - [ ] Add progress callback examples
  - [ ] Document performance characteristics:
    - Load time: <5 sec for 100K vectors
    - Memory usage: <200 MB for 10 chunks cached
    - Search latency: <100ms with warm cache

**Notes**:

#### 7.3 Create Performance Tuning Guide (Day 16 - Evening)

- [ ] **Create**: `docs/PERFORMANCE_TUNING.md` (new file, max 500 lines)
  - [ ] Chunk size recommendations
    - Small datasets (<50K): Use default 10K
    - Large datasets (500K+): Consider 25K chunks
  - [ ] Cache size tuning
    - Formula: `cache_size_mb = chunk_count_to_cache × 15 MB`
    - Recommendation: Cache 5-10 chunks (75-150 MB)
  - [ ] Encryption performance impact
    - Minimal: <5% overhead via S5.js
  - [ ] Search optimization strategies
    - Preload likely-needed chunks
    - Use larger nprobe for IVF (more chunks loaded)
  - [ ] Memory profiling tools
  - [ ] Benchmarking guide

**Notes**:

#### 7.4 Update README (Day 17 - Morning)

- [ ] **Modify**: `README.md`
  - [ ] Update feature list (chunked storage)
  - [ ] Update performance metrics
  - [ ] Add encryption-by-default badge
  - [ ] Update quick start examples

**Notes**:

---

## Success Criteria

**Functional Requirements**:
- [x] Support 1M+ vectors (vs. 500K previously)
- [x] Load time <5 seconds for 100K vectors (vs. ~30 sec)
- [x] Search latency <100ms with warm cache
- [x] Memory usage <200 MB for 100K vectors (10 chunks cached)
- [x] Encryption ON by default (XChaCha20-Poly1305)

**Code Quality**:
- [x] All tests pass (unit + integration + E2E)
- [x] Test coverage >80% for new code
- [x] All files within max line limits
- [x] No clippy warnings
- [x] Documentation complete

**Performance Benchmarks**:
- [x] Load time: 6x faster than monolithic format
- [x] Memory usage: 50% reduction (chunked vs. monolithic)
- [x] Search latency: <5% overhead vs. eager loading
- [x] Cache hit rate >70% for typical workloads

---

## Risk Mitigation

**Complexity Risk**:
- **Mitigation**: Strict TDD with bounded autonomy (max line counts)
- **Mitigation**: Small sub-phases (<1 day each)
- **Mitigation**: Integration tests at each phase

**Performance Risk**:
- **Mitigation**: Benchmark at Phase 6 before finalizing
- **Mitigation**: Profiling to validate memory bounds
- **Mitigation**: Fallback: Adjust chunk size if needed

**Compatibility Risk**:
- **Mitigation**: Clean break (no migration) - MVP doesn't use RAG yet
- **Mitigation**: Version in manifest for future compatibility

---

## Notes & Decisions

### Decision Log

**2025-01-XX**: Chose 10K vectors/chunk based on:
- Balance between API call overhead and granularity
- ~15 MB per chunk (reasonable S5 payload)
- LRU cache can hold 10 chunks (~150 MB) comfortably

**2025-01-XX**: Chose encryption ON by default:
- Aligns with Platformless AI privacy-first USP
- Minimal performance overhead via S5.js
- Users can opt-out if needed

**2025-01-XX**: Chose no backward compatibility:
- MVP doesn't use RAG yet (no existing data)
- Clean break simplifies implementation
- Faster delivery (no migration code)

### Open Questions

- [ ] Should we add chunk compression (in addition to encryption)?
  - Potential: 2-3x size reduction
  - Trade-off: CPU overhead on load

- [ ] Should we support adaptive chunk sizing?
  - Small chunks for recent data, large chunks for historical
  - Adds complexity

### Issues Tracker

_Track blockers and resolutions here_

---

## Estimated Timeline

- **Phase 1**: 3-4 days (Core chunking infrastructure)
- **Phase 2**: 4-5 days (Chunked persistence)
- **Phase 3**: 2-3 days (S5 encryption)
- **Phase 4**: 5-6 days (HNSW/IVF lazy loading)
- **Phase 5**: 3-4 days (Node.js bindings)
- **Phase 6**: 2-3 days (Testing & benchmarks)
- **Phase 7**: 2 days (Documentation)

**Total**: 21-27 days (~3-4 weeks)

**Buffer**: Add 20% for unexpected issues → **4-5 weeks total**

---

## Related Documents

- `docs/IMPLEMENTATION.md` - Original implementation plan
- `docs/VECTOR_DB_NODE_BINDINGS.md` - Node.js bindings spec
- `docs/sdk-reference/VECTOR_DB_INTEGRATION.md` - Integration guide
- `docs/s5js-reference/API.md` - S5.js encryption API
