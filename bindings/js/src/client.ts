import axios, { AxiosInstance, AxiosError } from 'axios';
import EventSource from 'eventsource';
import {
  ClientConfig,
  VectorData,
  VectorResult,
  SearchQuery,
  SearchResponse,
  BatchInsertResult,
  HealthStatus,
  Statistics,
  MigrationResult,
  RebalanceResult,
  BackupOptions,
  BackupResult,
  StreamUpdate,
} from './types';
import { mapAxiosError, isRetryableError } from './errors';

export class VectorDbClient {
  private axios: AxiosInstance;
  private eventSource?: EventSource;
  public config: ClientConfig;
  
  // For testing purposes
  public _mockNetworkError?: boolean;

  constructor(config: ClientConfig) {
    this.config = {
      timeout: 30000,
      maxRetries: 3,
      ...config,
    };

    this.axios = axios.create({
      baseURL: this.config.baseUrl,
      timeout: this.config.timeout,
      headers: {
        'Content-Type': 'application/json',
        ...(this.config.authToken && { 'Authorization': `Bearer ${this.config.authToken}` }),
      },
    });

    // Add request interceptor for retry logic
    this.axios.interceptors.response.use(
      response => response,
      async error => {
        if (this._mockNetworkError) {
          throw new Error('ECONNREFUSED');
        }
        
        const originalRequest = error.config;
        
        if (!originalRequest._retry && isRetryableError(error) && originalRequest._retryCount < this.config.maxRetries!) {
          originalRequest._retry = true;
          originalRequest._retryCount = (originalRequest._retryCount || 0) + 1;
          
          if (this.config.onRetry) {
            this.config.onRetry(originalRequest._retryCount, error);
          }
          
          // Exponential backoff
          const delay = Math.min(1000 * Math.pow(2, originalRequest._retryCount - 1), 10000);
          await new Promise(resolve => setTimeout(resolve, delay));
          
          return this.axios(originalRequest);
        }
        
        throw mapAxiosError(error);
      }
    );
  }

  async health(): Promise<HealthStatus> {
    const response = await this.axios.get<HealthStatus>('/health');
    return response.data;
  }

  async insertVector(data: VectorData): Promise<VectorResult> {
    const response = await this.axios.post<VectorResult>('/vectors', data);
    return response.data;
  }

  async getVector(id: string): Promise<VectorData> {
    const response = await this.axios.get<VectorData>(`/vectors/${id}`);
    return response.data;
  }

  async updateVector(data: VectorData): Promise<VectorResult> {
    const response = await this.axios.put<VectorResult>(`/vectors/${data.id}`, data);
    return response.data;
  }

  async deleteVector(id: string): Promise<void> {
    await this.axios.delete(`/vectors/${id}`);
  }

  async batchInsert(vectors: VectorData[]): Promise<BatchInsertResult> {
    const response = await this.axios.post<BatchInsertResult>('/vectors/batch', { vectors });
    return response.data;
  }

  async search(query: SearchQuery): Promise<SearchResponse> {
    const response = await this.axios.post<SearchResponse>('/search', query);
    return response.data;
  }

  async getStatistics(): Promise<Statistics> {
    const response = await this.axios.get<Statistics>('/admin/statistics');
    return response.data;
  }

  async triggerMigration(): Promise<MigrationResult> {
    const response = await this.axios.post<MigrationResult>('/admin/migrate');
    return response.data;
  }

  async rebalanceIndex(): Promise<RebalanceResult> {
    const response = await this.axios.post<RebalanceResult>('/admin/rebalance');
    return response.data;
  }

  async createBackup(options: BackupOptions): Promise<BackupResult> {
    const response = await this.axios.post<BackupResult>('/admin/backup', options);
    return response.data;
  }

  onUpdate(
    onMessage: (update: StreamUpdate) => void,
    onError?: (error: Error) => void
  ): () => void {
    const url = `${this.config.baseUrl}/updates`;
    
    this.eventSource = new EventSource(url, {
      headers: this.config.authToken ? { 'Authorization': `Bearer ${this.config.authToken}` } : {},
    });

    this.eventSource.onmessage = (event) => {
      try {
        const update = JSON.parse(event.data) as StreamUpdate;
        onMessage(update);
      } catch (error) {
        if (onError) {
          onError(error as Error);
        }
      }
    };

    this.eventSource.onerror = (error) => {
      if (onError) {
        onError(new Error('EventSource error'));
      }
    };

    // Return unsubscribe function
    return () => {
      if (this.eventSource) {
        this.eventSource.close();
        this.eventSource = undefined;
      }
    };
  }

  // For testing purposes
  _closeStream(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = undefined;
    }
  }
}