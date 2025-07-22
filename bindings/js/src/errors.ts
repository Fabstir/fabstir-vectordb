export class VectorDbError extends Error {
  constructor(message: string, public code?: string, public statusCode?: number) {
    super(message);
    this.name = 'VectorDbError';
  }
}

export class ConnectionError extends VectorDbError {
  constructor(message: string, public originalError?: Error) {
    super(message, 'CONNECTION_ERROR');
    this.name = 'ConnectionError';
  }
}

export class ValidationError extends VectorDbError {
  constructor(message: string) {
    super(message, 'VALIDATION_ERROR', 400);
    this.name = 'ValidationError';
  }
}

export class NotFoundError extends VectorDbError {
  constructor(message: string) {
    super(message, 'NOT_FOUND', 404);
    this.name = 'NotFoundError';
  }
}

export class DuplicateError extends VectorDbError {
  constructor(message: string) {
    super(message, 'DUPLICATE_ERROR', 409);
    this.name = 'DuplicateError';
  }
}

export class TimeoutError extends VectorDbError {
  constructor(message: string) {
    super(message, 'TIMEOUT_ERROR', 408);
    this.name = 'TimeoutError';
  }
}

export function isRetryableError(error: any): boolean {
  if (error.code === 'ECONNREFUSED' || error.code === 'ECONNRESET' || error.code === 'ETIMEDOUT') {
    return true;
  }
  if (error.response?.status >= 500) {
    return true;
  }
  if (error.response?.status === 429) {
    return true;
  }
  return false;
}

export function mapAxiosError(error: any): VectorDbError {
  if (error.code === 'ECONNREFUSED' || error.code === 'ECONNRESET') {
    return new ConnectionError(`Network error: ${error.message}`, error);
  }
  
  if (error.code === 'ETIMEDOUT' || error.code === 'ECONNABORTED') {
    return new TimeoutError(`Request timeout: ${error.message}`);
  }
  
  if (error.response) {
    const status = error.response.status;
    const message = error.response.data?.error || error.response.data?.message || error.message;
    
    switch (status) {
      case 400:
        return new ValidationError(message);
      case 404:
        return new NotFoundError(message);
      case 409:
        return new DuplicateError(message);
      case 408:
        return new TimeoutError(message);
      default:
        return new VectorDbError(message, 'HTTP_ERROR', status);
    }
  }
  
  return new VectorDbError(error.message || 'Unknown error');
}