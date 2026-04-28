/**
 * HTTP Interceptor
 * Provides functionality for intercepting requests, responses, and errors.
 */

/**
 * HTTP Request configuration
 */
export interface RequestConfig {
  url?: string;
  method?: string;
  headers?: Record<string, string>;
  body?: unknown;
  [key: string]: unknown;
}

/**
 * HTTP Response object
 */
export interface ResponseData {
  status: number;
  statusText?: string;
  headers?: Record<string, string>;
  data?: unknown;
  [key: string]: unknown;
}

/**
 * Request Interceptor Interface
 */
export interface RequestInterceptor {
  /**
   * Intercepting a request
   * @param config Request configuration
   * @returns Processed request configuration
   */
  intercept(config: RequestConfig): RequestConfig | Promise<RequestConfig>;
}

/**
 * Response interceptor interface
 */
export interface ResponseInterceptor {
  /**
   * Intercept the response
   * @param response The response object
   * @returns The processed response object
   */
  intercept(response: ResponseData): ResponseData | Promise<ResponseData>;
}

/**
 * Error Interceptor Interface
 */
export interface ErrorInterceptor {
  /**
   * Intercepting errors
   * @param error The error object
   * @returns The processed error object
   */
  intercept(error: Error): Error | Promise<Error>;
}

/**
 * Interceptor Manager
 */
export class InterceptorManager {
  private requestInterceptors: RequestInterceptor[] = [];
  private responseInterceptors: ResponseInterceptor[] = [];
  private errorInterceptors: ErrorInterceptor[] = [];

  /**
   * Add a request interceptor
   */
  addRequestInterceptor(interceptor: RequestInterceptor): void {
    this.requestInterceptors.push(interceptor);
  }

  /**
   * Add a response interceptor
   */
  addResponseInterceptor(interceptor: ResponseInterceptor): void {
    this.responseInterceptors.push(interceptor);
  }

  /**
   * Add an error interceptor
   */
  addErrorInterceptor(interceptor: ErrorInterceptor): void {
    this.errorInterceptors.push(interceptor);
  }

  /**
   * Apply request interceptors
   */
  async applyRequestInterceptors(config: RequestConfig): Promise<RequestConfig> {
    let processedConfig = config;
    for (const interceptor of this.requestInterceptors) {
      processedConfig = await interceptor.intercept(processedConfig);
    }
    return processedConfig;
  }

  /**
   * Apply response interceptors
   */
  async applyResponseInterceptors(response: ResponseData): Promise<ResponseData> {
    let processedResponse = response;
    for (const interceptor of this.responseInterceptors) {
      processedResponse = await interceptor.intercept(processedResponse);
    }
    return processedResponse;
  }

  /**
   * Application error interceptor
   */
  async applyErrorInterceptors(error: Error): Promise<Error> {
    let processedError = error;
    for (const interceptor of this.errorInterceptors) {
      processedError = await interceptor.intercept(processedError);
    }
    return processedError;
  }

  /**
   * Clear all interceptors
   */
  clear(): void {
    this.requestInterceptors = [];
    this.responseInterceptors = [];
    this.errorInterceptors = [];
  }
}

/**
 * Common interceptor factory functions
 */

/**
 * Create an authentication interceptor
 */
export function createAuthInterceptor(
  token: string,
  scheme: string = "Bearer",
): RequestInterceptor {
  return {
    intercept(config: RequestConfig) {
      return {
        ...config,
        headers: {
          ...config.headers,
          Authorization: `${scheme} ${token}`,
        },
      };
    },
  };
}

/**
 * Create a log interceptor
 */
export function createLoggingInterceptor(logger: (message: string, data?: unknown) => void): {
  request: RequestInterceptor;
  response: ResponseInterceptor;
  error: ErrorInterceptor;
} {
  return {
    request: {
      intercept(config: RequestConfig) {
        logger(`[Request] ${config.method} ${config.url}`, config);
        return config;
      },
    },
    response: {
      intercept(response: ResponseData) {
        logger(`[Response] ${response.status}`, response);
        return response;
      },
    },
    error: {
      intercept(error: Error) {
        logger(`[Error] ${error.message}`, error);
        return error;
      },
    },
  };
}

/**
 * Error with retryable flag
 */
interface RetryableError extends Error {
  retryable?: boolean;
}

/**
 * Create a retry interceptor
 */
export function createRetryInterceptor(
  _shouldRetry: (error: Error, retryCount: number) => boolean,
  _getRetryDelay: (retryCount: number) => number,
): ErrorInterceptor {
  return {
    async intercept(error: Error) {
      // This interceptor needs to be used in conjunction with external retry logic.
      // This is just to indicate whether the error can be retried.
      (error as RetryableError).retryable = true;
      return error;
    },
  };
}
