/**
 * REST Executor Type Definition
 */

// Import basic types from @wf-agent/types
import type { HTTPMethod, HttpResponse } from "@wf-agent/types";
// Import the interceptor type from @wf-agent/common-utils.
import type {
  RequestInterceptor,
  ResponseInterceptor,
  ErrorInterceptor,
} from "@wf-agent/common-utils";

/**
 * HTTP Request Configuration
 */
export interface HttpRequestConfig {
  /** Request URL */
  url: string;
  /** HTTP Methods */
  method: HTTPMethod;
  /** Request headers */
  headers?: Record<string, string>;
  /** Request Body */
  body?: unknown;
  /** Query parameters */
  query?: Record<string, unknown>;
  /** Timeout period (in milliseconds) */
  timeout?: number;
  /** Basic URL */
  baseUrl?: string;
}

// Reexport the types to maintain backward compatibility.
export type { HttpResponse, RequestInterceptor, ResponseInterceptor, ErrorInterceptor };

/**
 * REST Executor Configuration
 */
export interface RestExecutorConfig {
  /** Basic URL */
  baseUrl?: string;
  /** Default request headers */
  headers?: Record<string, string>;
  /** Default timeout period (in milliseconds) */
  timeout?: number;
  /** Request interceptor */
  requestInterceptors?: RequestInterceptor[];
  /** Response interceptor */
  responseInterceptors?: ResponseInterceptor[];
  /** Error interceptor */
  errorInterceptors?: ErrorInterceptor[];
  /** Whether to enable the circuit breaker. */
  enableCircuitBreaker?: boolean;
  /** Circuit Breaker Configuration */
  circuitBreaker?: {
    /** Failure threshold */
    failureThreshold: number;
    /** Reset timeout (in milliseconds) */
    resetTimeout: number;
    /** Number of semi-open state requests */
    halfOpenRequests: number;
  };
}
