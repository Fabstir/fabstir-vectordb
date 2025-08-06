# Changelog

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
