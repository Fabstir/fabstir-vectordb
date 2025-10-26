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
