/**
 * HTTP Client
 *
 * Provides a unified HTTP request interface, integrating features such as retry, circuit breaking, and rate limiting.
 */

import { now, diffTimestamp } from "../utils/timestamp-utils.js";
import type { HttpClientConfig, HttpRequestOptions, HttpResponse } from "@wf-agent/types";
import { TimeoutError, CircuitBreakerOpenError, HttpError } from "@wf-agent/types";
import {
  BadRequestError,
  UnauthorizedError,
  ForbiddenError,
  NotFoundHttpError,
  ConflictError,
  UnprocessableEntityError,
  InternalServerError,
  ServiceUnavailableError,
  RateLimitError,
} from "./errors.js";
import { executeWithRetry, type RetryConfig } from "./retry-handler.js";
import { CircuitBreaker } from "./circuit-breaker.js";
import { RateLimiter } from "./rate-limiter.js";
import { isAbortError } from "../error/error-utils.js";

/**
 * HTTP Client
 */
export class HttpClient {
  private readonly config: HttpClientConfig;
  private readonly retryConfig: RetryConfig;
  private readonly circuitBreaker?: CircuitBreaker;
  private readonly rateLimiter?: RateLimiter;

  constructor(config: HttpClientConfig = {}) {
    this.config = {
      baseURL: config.baseURL || "",
      defaultHeaders: config.defaultHeaders || {
        "Content-Type": "application/json",
      },
      timeout: config.timeout || 30000,
      maxRetries: config.maxRetries || 3,
      retryDelay: config.retryDelay || 1000,
      enableCircuitBreaker: config.enableCircuitBreaker || false,
      enableRateLimiter: config.enableRateLimiter || false,
      circuitBreakerFailureThreshold: config.circuitBreakerFailureThreshold || 5,
      rateLimiterCapacity: config.rateLimiterCapacity || 60,
      rateLimiterRefillRate: config.rateLimiterRefillRate || 10,
      logger: config.logger,
    };

    this.retryConfig = {
      maxRetries: this.config.maxRetries || 3,
      baseDelay: this.config.retryDelay || 1000,
    };

    if (this.config.enableCircuitBreaker) {
      this.circuitBreaker = new CircuitBreaker({
        failureThreshold: this.config.circuitBreakerFailureThreshold || 5,
      });
    }

    if (this.config.enableRateLimiter) {
      this.rateLimiter = new RateLimiter({
        capacity: this.config.rateLimiterCapacity || 60,
        refillRate: this.config.rateLimiterRefillRate || 10,
      });
    }
  }

  /**
   * GET request
   */
  async get<T = unknown>(url: string, options?: HttpRequestOptions): Promise<HttpResponse<T>> {
    return this.request<T>({ ...options, method: "GET", url });
  }

  /**
   * POST request
   */
  async post<T = unknown>(
    url: string,
    body?: unknown,
    options?: HttpRequestOptions,
  ): Promise<HttpResponse<T>> {
    return this.request<T>({ ...options, method: "POST", url, body });
  }

  /**
   * PUT request
   */
  async put<T = unknown>(
    url: string,
    body?: unknown,
    options?: HttpRequestOptions,
  ): Promise<HttpResponse<T>> {
    return this.request<T>({ ...options, method: "PUT", url, body });
  }

  /**
   * DELETE request
   */
  async delete<T = unknown>(url: string, options?: HttpRequestOptions): Promise<HttpResponse<T>> {
    return this.request<T>({ ...options, method: "DELETE", url });
  }

  /**
   * Common Request Methods
   */
  protected async request<T = unknown>(options: HttpRequestOptions): Promise<HttpResponse<T>> {
    // Check throttling limits.
    if (this.rateLimiter) {
      await this.rateLimiter.waitForToken();
    }

    // Check the fuse.
    if (this.circuitBreaker && this.circuitBreaker.isOpen()) {
      throw new CircuitBreakerOpenError("Circuit breaker is OPEN", this.circuitBreaker.getState());
    }

    // Execute the request (with retries)
    try {
      const result = await executeWithRetry(
        () => this.executeRequest<T>(options),
        this.retryConfig,
      );

      // Record successful.
      if (this.circuitBreaker) {
        this.circuitBreaker.execute(async () => result);
      }

      return result;
    } catch (error) {
      // Record of failure
      if (this.circuitBreaker) {
        try {
          await this.circuitBreaker.execute(async () => {
            throw error;
          });
        } catch {
          // Ignore the errors from the fuse.
        }
      }

      throw error;
    }
  }

  /**
   * Log recording auxiliary methods
   */
  private log(level: keyof import("@wf-agent/types").HttpLogger, msg: string, context?: unknown) {
    if (!this.config.logger?.[level]) return;
    this.config.logger[level]!(msg, context);
  }

  /**
   * Execute the actual HTTP request.
   */
  private async executeRequest<T = unknown>(options: HttpRequestOptions): Promise<HttpResponse<T>> {
    const url = this.buildURL(options.url || "", options.query);
    const method = options.method || "GET";
    const timeout = options.timeout || this.config.timeout;
    const startTime = now();

    this.log("info", `[HTTP] ${method} ${url} starting`);

    // Merge request headers
    const headers = {
      ...this.config.defaultHeaders,
      ...options.headers,
    };

    // Construct a request body
    let body: string | undefined;
    if (options.body !== undefined) {
      if (typeof options.body === "string") {
        body = options.body;
      } else {
        body = JSON.stringify(options.body);
      }
    }

    // Create an AbortController for use in case of timeouts.
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout);

    // Merge external signal if provided
    if (options.signal) {
      options.signal.addEventListener("abort", () => controller.abort());
    }

    try {
      const response = await fetch(url, {
        method,
        headers,
        body,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);
      const duration = diffTimestamp(startTime, now());

      // Check the response status.
      if (!response.ok) {
        const errorText = await response.text();
        this.log(
          "warn",
          `[HTTP] ${method} ${url} failed with ${response.status} in ${duration}ms`,
          { status: response.status, error: errorText },
        );
        throw this.createHttpError(response.status, errorText, options.url);
      }

      this.log("debug", `[HTTP] ${method} ${url} succeeded in ${duration}ms`, {
        status: response.status,
      });

      // If the request specifies a streaming response, return the stream instead of parsing the data.
      if (options.stream) {
        return {
          data: response.body as T, // Return the response stream
          status: response.status,
          statusText: response.statusText,
          headers: this.headersToObject(response.headers),
          requestId: response.headers.get("x-request-id") || undefined,
        };
      }

      // Parse the response.
      let data: T;
      const contentType = response.headers.get("content-type");
      if (contentType && contentType.includes("application/json")) {
        data = (await response.json()) as T;
      } else {
        data = (await response.text()) as T;
      }

      return {
        data,
        status: response.status,
        statusText: response.statusText,
        headers: this.headersToObject(response.headers),
        requestId: response.headers.get("x-request-id") || undefined,
      };
    } catch (error) {
      clearTimeout(timeoutId);
      const duration = diffTimestamp(startTime, now());

      if (isAbortError(error)) {
        this.log("error", `[HTTP] ${method} ${url} timeout after ${duration}ms`, { timeout });
        throw new TimeoutError(`Request timeout after ${timeout}ms`, timeout || 30000, {
          url: options.url,
        });
      }

      this.log("error", `[HTTP] ${method} ${url} error after ${duration}ms`, {
        error: String(error),
      });
      throw error;
    }
  }

  /**
   * Construct a complete URL
   */
  private buildURL(url: string, query?: Record<string, string | number | boolean>): string {
    let fullURL = url;

    if (this.config.baseURL && !url.startsWith("http")) {
      fullURL = this.config.baseURL + url;
    }

    if (query && Object.keys(query).length > 0) {
      const queryString = Object.entries(query)
        .map(([key, value]) => `${key}=${encodeURIComponent(String(value))}`)
        .join("&");
      fullURL += (fullURL.includes("?") ? "&" : "?") + queryString;
    }

    return fullURL;
  }

  /**
   * Create an HTTP error
   */
  private createHttpError(status: number, message: string, url?: string): Error {
    const context = { url, status };

    switch (status) {
      case 400:
        return new BadRequestError(`Bad request: ${message}`, context);

      case 401:
        return new UnauthorizedError(`Unauthorized: ${message}`, context);

      case 403:
        return new ForbiddenError(`Forbidden: ${message}`, context);

      case 404:
        return new NotFoundHttpError(`Not found: ${message}`, url || "", context);

      case 409:
        return new ConflictError(`Conflict: ${message}`, context);

      case 422:
        return new UnprocessableEntityError(`Unprocessable entity: ${message}`, context);

      case 429:
        return new RateLimitError(`Rate limit exceeded: ${message}`, undefined, context);

      case 500:
        return new InternalServerError(`Internal server error: ${message}`, context);

      case 503:
        return new ServiceUnavailableError(`Service unavailable: ${message}`, context);

      default:
        return new HttpError(`HTTP ${status}: ${message}`, status, context);
    }
  }

  /**
   * Convert Headers to an object
   */
  private headersToObject(headers: Headers): Record<string, string> {
    const obj: Record<string, string> = {};
    headers.forEach((value, key) => {
      obj[key] = value;
    });
    return obj;
  }
}
