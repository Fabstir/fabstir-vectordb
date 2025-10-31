# Changelog

## [0.2.0] - 2025-01-31

### Added
- **Full CRUD Operations**: Delete vectors by ID or metadata, update metadata in-place
- **Metadata Filtering**: MongoDB-style query language with 8 operators (Equals, `$in`, `$gt`, `$gte`, `$lt`, `$lte`, `$and`, `$or`)
- **Soft Deletion**: Mark vectors as deleted, physically remove on save (vacuum)
- **Batch Deletion**: `deleteByMetadata()` with filter support and result tracking
- **Filtered Search**: Search with metadata filters using post-search filtering strategy
- **New API Methods**:
  - `deleteVector(id: string): Promise<void>` - Delete single vector
  - `deleteByMetadata(filter: any): Promise<DeleteResult>` - Bulk delete with filter
  - `updateMetadata(id: string, metadata: any): Promise<void>` - Update metadata
  - `search(vector, k, { filter }): Promise<SearchResult[]>` - Search with filters
- **DeleteResult Interface**: Returns `{ deletedCount, deletedIds }` for tracking
- **E2E Integration Tests**: Comprehensive CRUD workflow validation (3/3 passing)
- **Performance Documentation**: Filter optimization and vacuum strategies

### Changed
- **Manifest Format**: Upgraded from v2 to v3 (includes soft deletion tracking)
- **Auto-Migration**: v2 manifests automatically upgrade on first load
- **Search Strategy**: Post-filtering with k_oversample (3x multiplier) for filtered queries
- **Documentation**: Updated all docs with v0.2.0 CRUD examples and best practices

### Performance
- Post-search filtering with minimal impact (<10ms for selective filters)
- Soft deletions have zero search penalty (filtered during results)
- Vacuum on save (+200-500ms for physical deletion)
- Tested with 100K vectors, complex filters, <100ms search latency

### Breaking Changes
- Manifest format v2 → v3 (auto-migrated on load, no action required)

## [Unreleased]

### Added
- Environment variable configuration for S5 backend URLs
- Support for STORAGE_MODE (mock/real) selection
- Dynamic storage backend configuration
- Improved health endpoint with actual storage status

### Changed
- Replaced hardcoded localhost:5524 with S5_MOCK_SERVER_URL
- Updated storage factory to support multiple backends
- Enhanced health reporting with storage configuration

### Fixed
- Port configuration issue preventing flexible deployment
- Storage backend connection for docker deployments

### Environment Variables
- `STORAGE_MODE`: Select mock or real S5 backend
- `S5_MOCK_SERVER_URL`: Configure S5 backend URL
- `S5_NODE_URL`: Real S5 portal URL
- `DATABASE_URL`: PostgreSQL connection string

## Phase 4.3.1 Integration
Successfully integrated with Enhanced S5.js server for the Fabstir LLM Node project.
