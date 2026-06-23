/**
 * Circuit Breaker Pattern Implementation
 * Prevents cascading failures by detecting repeated failures and temporarily blocking requests
 *
 * This is a generic implementation suitable for:
 * - HTTP requests
 * - Database connections
 * - File I/O operations
 * - Any external service calls
 */

import { now } from "./timestamp-utils.js";
import { getGlobalLogger } from "@wf-agent/common-utils";

// Get circuit breaker module logger
const logger = getGlobalLogger().child("circuit-breaker", { pkg: "common-utils" });

/**
 * Circuit breaker state
 */
export type CircuitState = "CLOSED" | "OPEN" | "HALF_OPEN";

/**
 * Circuit breaker configuration
 */
export interface CircuitBreakerConfig {
  /** Number of failures before opening circuit (default: 5) */
  failureThreshold?: number;
  /** Time in ms before attempting recovery (default: 60000) */
  resetTimeout?: number;
  /** Number of successful calls needed to close circuit from half-open (default: 3) */
  successThreshold?: number;
  /** Monitor function for logging/metrics (optional) */
  monitor?: (state: CircuitState, metrics: CircuitMetrics) => void;
}

/**
 * Circuit breaker metrics
 */
export interface CircuitMetrics {
  state: CircuitState;
  failureCount: number;
  successCount: number;
  lastFailureTime: number | null;
  lastSuccessTime: number | null;
  totalRequests: number;
  totalFailures: number;
  totalSuccesses: number;
}

/**
 * Circuit Breaker - prevents cascading failures
 *
 * States:
 * - CLOSED: Normal operation, requests pass through
 * - OPEN: Circuit tripped, requests fail immediately
 * - HALF_OPEN: Testing if service recovered, limited requests allowed
 *
 * @example
 * ```typescript
 * const breaker = new CircuitBreaker({
 *   failureThreshold: 5,
 *   resetTimeout: 30000,
 * });
 *
 * try {
 *   const result = await breaker.execute(() => httpClient.get(url));
 *   console.log('Success:', result);
 * } catch (error) {
 *   if (error.message.includes('Circuit breaker is OPEN')) {
 *     // Handle circuit open scenario
 *   }
 * }
 * ```
 *
 * @example
 * ```typescript
 * // Use with database operations
 * const dbBreaker = new CircuitBreaker({
 *   failureThreshold: 3,
 *   resetTimeout: 10000,
 *   monitor: (state, metrics) => {
 *     console.log(`Circuit state: ${state}`, metrics);
 *   }
 * });
 *
 * const data = await dbBreaker.execute(() => storage.load(id));
 * ```
 */
export class CircuitBreaker {
  private state: CircuitState = "CLOSED";
  private failureCount = 0;
  private successCount = 0;
  private lastFailureTime: number | null = null;
  private lastSuccessTime: number | null = null;
  private nextAttempt: number = 0;
  private totalRequests = 0;
  private totalFailures = 0;
  private totalSuccesses = 0;

  private readonly failureThreshold: number;
  private readonly resetTimeout: number;
  private readonly successThreshold: number;
  private readonly monitor?: (state: CircuitState, metrics: CircuitMetrics) => void;

  constructor(config: CircuitBreakerConfig = {}) {
    this.failureThreshold = config.failureThreshold ?? 5;
    this.resetTimeout = config.resetTimeout ?? 60000;
    this.successThreshold = config.successThreshold ?? 3;
    this.monitor = config.monitor;
  }

  /**
   * Execute a function with circuit breaker protection
   */
  async execute<T>(fn: () => Promise<T>): Promise<T> {
    this.totalRequests++;

    // Check if circuit should transition from open to half-open
    if (this.state === "OPEN") {
      if (now() >= this.nextAttempt) {
        const metrics = this.getMetrics();
        logger.info("Circuit breaker transitioning to HALF_OPEN", { ...metrics });
        this.state = "HALF_OPEN";
        this.successCount = 0;
        this.notifyMonitor();
      } else {
        const timeUntilReset = Math.ceil((this.nextAttempt - now()) / 1000);
        logger.warn("Circuit breaker is OPEN, request rejected", {
          timeUntilResetSeconds: timeUntilReset,
        });
        throw new Error(`Circuit breaker is OPEN. Try again in ${timeUntilReset}s`);
      }
    }

    try {
      const result = await fn();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure(error as Error);
      throw error;
    }
  }

  /**
   * Check if the circuit is open
   */
  isOpen(): boolean {
    if (this.state === "OPEN") {
      // Check if it's possible to attempt a recovery
      if (now() >= this.nextAttempt) {
        this.state = "HALF_OPEN";
        this.successCount = 0;
        return false;
      }
      return true;
    }
    return false;
  }

  /**
   * Get current circuit state
   */
  getState(): CircuitState {
    return this.state;
  }

  /**
   * Get current circuit breaker metrics
   */
  getMetrics(): CircuitMetrics {
    return {
      state: this.state,
      failureCount: this.failureCount,
      successCount: this.successCount,
      lastFailureTime: this.lastFailureTime,
      lastSuccessTime: this.lastSuccessTime,
      totalRequests: this.totalRequests,
      totalFailures: this.totalFailures,
      totalSuccesses: this.totalSuccesses,
    };
  }

  /**
   * Manually reset the circuit breaker
   */
  reset(): void {
    logger.info("Circuit breaker manually reset");
    this.state = "CLOSED";
    this.failureCount = 0;
    this.successCount = 0;
    this.nextAttempt = 0;
    this.notifyMonitor();
  }

  /**
   * Force circuit to open state (for maintenance/testing)
   */
  forceOpen(): void {
    logger.warn("Circuit breaker forced open");
    this.state = "OPEN";
    this.lastFailureTime = Date.now();
    this.nextAttempt = now() + this.resetTimeout;
    this.notifyMonitor();
  }

  private onSuccess(): void {
    this.totalSuccesses++;
    this.lastSuccessTime = now();
    this.failureCount = 0;

    if (this.state === "HALF_OPEN") {
      this.successCount++;
      if (this.successCount >= this.successThreshold) {
        const metrics = this.getMetrics();
        logger.info("Circuit breaker closed after successful recovery", { ...metrics });
        this.state = "CLOSED";
        this.successCount = 0;
      }
    }

    this.notifyMonitor();
  }

  private onFailure(error: Error): void {
    this.totalFailures++;
    this.failureCount++;
    this.lastFailureTime = now();

    logger.error("Circuit breaker recorded failure", {
      error: error.message,
      failureCount: this.failureCount,
      threshold: this.failureThreshold,
    });

    if (this.state === "HALF_OPEN") {
      // Any failure in half-open state reopens circuit
      logger.warn("Circuit breaker reopened after failure in HALF_OPEN state");
      this.state = "OPEN";
      this.nextAttempt = now() + this.resetTimeout;
    } else if (this.failureCount >= this.failureThreshold) {
      // Threshold reached, open circuit
      logger.warn("Circuit breaker opened due to failure threshold", {
        failureCount: this.failureCount,
        threshold: this.failureThreshold,
      });
      this.state = "OPEN";
      this.nextAttempt = now() + this.resetTimeout;
    }

    this.notifyMonitor();
  }

  private notifyMonitor(): void {
    if (this.monitor) {
      try {
        this.monitor(this.state, this.getMetrics());
      } catch (error) {
        logger.error("Monitor function threw error", { error });
      }
    }
  }
}

/**
 * Create a circuit breaker decorator for class methods
 *
 * @example
 * ```typescript
 * class StorageService {
 *   private breaker = new CircuitBreaker();
 *
 *   @circuitBreakerDecorator(breaker)
 *   async loadData(id: string): Promise<Data> {
 *     // ... implementation
 *   }
 * }
 * ```
 */
export function circuitBreakerDecorator(breaker: CircuitBreaker) {
  return function <T extends (...args: unknown[]) => Promise<unknown>>(
    _target: object,
    _propertyKey: string,
    descriptor: TypedPropertyDescriptor<T>,
  ): TypedPropertyDescriptor<T> {
    const originalMethod = descriptor.value!;

    descriptor.value = async function (this: unknown, ...args: unknown[]): Promise<unknown> {
      return breaker.execute(() => originalMethod.apply(this, args));
    } as T;

    return descriptor;
  };
}
