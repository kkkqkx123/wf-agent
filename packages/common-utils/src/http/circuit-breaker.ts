/**
 * Circuit Breaker
 *
 * Prevents cascading failures by activating the circuit breaker when the number of failures exceeds a threshold.
 */

import { now } from "../utils/timestamp-utils.js";

/**
 * Circuit Breaker Status
 */
type CircuitState =
  | "CLOSED" /** Closed state */
  | "OPEN" /** Open status */
  | "HALF_OPEN"; /** Semi-open state */

/**
 * Circuit Breaker Configuration
 */
export interface CircuitBreakerConfig {
  /** Failure threshold */
  failureThreshold: number;
  /** Success threshold (used for transitioning from HALF_OPEN to CLOSED) */
  successThreshold?: number;
  /** Reset the timeout period (in milliseconds) */
  resetTimeout?: number;
}

/**
 * Circuit Breaker
 */
export class CircuitBreaker {
  private state: CircuitState = "CLOSED";
  private failureCount: number = 0;
  private successCount: number = 0;
  private nextAttempt: number = 0;

  private readonly failureThreshold: number;
  private readonly successThreshold: number;
  private readonly resetTimeout: number;

  constructor(config: CircuitBreakerConfig) {
    this.failureThreshold = config.failureThreshold;
    this.successThreshold = config.successThreshold || 3;
    this.resetTimeout = config.resetTimeout || 60000; // Default is 60 seconds.
  }

  /**
   * Execute the function (with circuit breaker protection)
   */
  async execute<T>(fn: () => Promise<T>): Promise<T> {
    if (this.isOpen()) {
      throw new Error("Circuit breaker is OPEN");
    }

    try {
      const result = await fn();
      this.recordSuccess();
      return result;
    } catch (error) {
      this.recordFailure();
      throw error;
    }
  }

  /**
   * Check if the fuse is turned on.
   */
  isOpen(): boolean {
    if (this.state === "OPEN") {
      // Check if it's possible to attempt a recovery.
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
   * Get the status
   */
  getState(): string {
    return this.state;
  }

  /**
   * Reset the fuse.
   */
  reset(): void {
    this.state = "CLOSED";
    this.failureCount = 0;
    this.successCount = 0;
    this.nextAttempt = 0;
  }

  /**
   * Record successful.
   */
  private recordSuccess(): void {
    this.failureCount = 0;

    if (this.state === "HALF_OPEN") {
      this.successCount++;
      if (this.successCount >= this.successThreshold) {
        this.state = "CLOSED";
      }
    }
  }

  /**
   * Record of failure
   */
  private recordFailure(): void {
    this.failureCount++;

    // Failed in the HALF_OPEN state; immediately reopen the fuse.
    if (this.state === "HALF_OPEN") {
      this.state = "OPEN";
      this.nextAttempt = now() + this.resetTimeout;
      return;
    }

    if (this.failureCount >= this.failureThreshold) {
      this.state = "OPEN";
      this.nextAttempt = now() + this.resetTimeout;
    }
  }
}
