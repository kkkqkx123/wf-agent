/**
 * HTTP module export
 */

// Exporting HTTP Error Types
export {
  BadRequestError,
  UnauthorizedError,
  ForbiddenError,
  NotFoundHttpError,
  ConflictError,
  UnprocessableEntityError,
  RateLimitError,
  InternalServerError,
  ServiceUnavailableError,
} from "./errors.js";

// Exporting HTTP Clients
export { HttpClient } from "./http-client.js";

// Export Transport Protocol
export {
  HttpTransport,
  SseTransport,
  Transport,
  TransportResponse,
  TransportOptions,
} from "./transport.js";

// Export Retry Processor
export { executeWithRetry } from "./retry-handler.js";
export { NonRetryableStatusCode } from "./retry-handler.js";
export type { RetryConfig } from "./retry-handler.js";

// Export Fuse
export { CircuitBreaker } from "./circuit-breaker.js";
export type { CircuitBreakerConfig } from "./circuit-breaker.js";

// Derived current limiter
export { RateLimiter } from "./rate-limiter.js";
export type { RateLimiterConfig } from "./rate-limiter.js";

// Export Interceptor
export {
  RequestInterceptor,
  ResponseInterceptor,
  ErrorInterceptor,
  InterceptorManager,
  createAuthInterceptor,
  createLoggingInterceptor,
  createRetryInterceptor,
} from "./interceptors.js";

// Export SSE Utilities
export { parseSSELine, parseSSELines, streamSSE, readSSEStream } from "./sse-utils.js";

// Export Type
export type {
  HTTPMethod,
  HttpRequestOptions,
  HttpResponse,
  HttpClientConfig,
} from "@wf-agent/types";
