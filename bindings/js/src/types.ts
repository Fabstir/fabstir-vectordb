export interface ClientConfig {
  baseUrl: string;
  timeout?: number;
  maxRetries?: number;
  authToken?: string;
  onRetry?: (attempt: number, error: Error) => void;
}

export interface VectorData {
  id: string;
  vector: number[];
  metadata?: Record<string, any>;
}

export interface VectorResult extends VectorData {
  index: string;
  timestamp: string;
}

export interface SearchOptions {
  searchRecent?: boolean;
  searchHistorical?: boolean;
  hnswEf?: number;
  scoreThreshold?: number;
  timeoutMs?: number;
  includeMetadata?: boolean;
}

export interface SearchQuery {
  vector: number[];
  k: number;
  filter?: Record<string, any>;
  options?: SearchOptions;
}

export interface SearchResult {
  id: string;
  distance: number;
  score: number;
  vector: number[];
  metadata?: Record<string, any>;
}

export interface SearchResponse {
  results: SearchResult[];
  searchTimeMs: number;
  indicesSearched: number;
}

export interface BatchInsertResult {
  successful: number;
  failed: number;
  errors: Array<{
    id: string;
    error: string;
  }>;
}

export interface HealthStatus {
  status: string;
  indices: {
    hnsw: {
      status: string;
      vectorCount: number;
    };
    ivf: {
      status: string;
      vectorCount: number;
      clusters: number;
    };
  };
  version: string;
}

export interface Statistics {
  totalVectors: number;
  recentVectors: number;
  historicalVectors: number;
  memoryUsage: {
    totalBytes: number;
    hnswBytes: number;
    ivfBytes: number;
  };
  performance: {
    averageSearchTimeMs: number;
    averageInsertTimeMs: number;
  };
}

export interface MigrationResult {
  vectorsMigrated: number;
  durationMs: number;
  errors: string[];
}

export interface RebalanceResult {
  clustersModified: number;
  vectorsMoved: number;
  finalVariance: number;
  durationMs: number;
}

export interface BackupOptions {
  path: string;
  compress?: boolean;
}

export interface BackupResult {
  backupSize: number;
  vectorsBackedUp: number;
  compressionRatio: number;
  path: string;
}

export type UpdateType = 'vector_inserted' | 'vector_updated' | 'vector_deleted' | 'migration_completed' | 'rebalance_completed';

export interface StreamUpdate {
  type: UpdateType;
  timestamp: string;
  data: any;
}