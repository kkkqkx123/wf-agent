/**
 * HTTP Type Definitions
 * Defines all types and interfaces required by HTTP clients
 */

/**
 * HTTP Method Types
 */
export type HTTPMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

/**
 * HTTP Logging Interface
 */
export interface HttpLogger {
  debug?(msg: string, context?: unknown): void;
  info?(msg: string, context?: unknown): void;
  warn?(msg: string, context?: unknown): void;
  error?(msg: string, context?: unknown): void;
}

/**
 * HTTP Request Options
 */
export interface HttpRequestOptions {
  /** Request URL */
  url?: string;
  /** Request method */
  method?: HTTPMethod;
  /** request header */
  headers?: Record<string, string>;
  /** requestor */
  body?: unknown;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Streaming response or not */
  stream?: boolean;
  /** Query parameters */
  query?: Record<string, string | number | boolean>;
  /** AbortSignal for request cancellation */
  signal?: AbortSignal;
}

/**
 * HTTP response
 */
export interface HttpResponse<T = unknown> {
  /** Response data */
  data: T;
  /** HTTP status code */
  status: number;
  /** Status text */
  statusText: string;
  /** response header */
  headers: Record<string, string>;
  /** Request ID */
  requestId?: string;
}

/**
 * HTTP Client Configuration
 */
export interface HttpClientConfig {
  /** Base URL */
  baseURL?: string;
  /** Default request header */
  defaultHeaders?: Record<string, string>;
  /** Default timeout in milliseconds */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Whether or not the fuse is enabled */
  enableCircuitBreaker?: boolean;
  /** Whether to enable the current limiter */
  enableRateLimiter?: boolean;
  /** Fuse Failure Threshold */
  circuitBreakerFailureThreshold?: number;
  /** Current Limiter Capacity */
  rateLimiterCapacity?: number;
  /** Restrictor fill rate (per second) */
  rateLimiterRefillRate?: number;
  /** Optional Logger */
  logger?: HttpLogger;
}
