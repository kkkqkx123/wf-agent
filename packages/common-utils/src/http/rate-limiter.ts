/**
 * current limiter
 *
 * Limits the request rate using a token bucket algorithm
 */

import { now } from "../utils/timestamp-utils.js";

/**
 * Current Limiter Arrangement
 */
export interface RateLimiterConfig {
  /** Token Bucket Capacity */
  capacity: number;
  /** Filling rate (per second) */
  refillRate: number;
}

/**
 * current limiter
 */
export class RateLimiter {
  private tokens: number;
  private lastRefill: number;

  private readonly capacity: number;
  private readonly refillRate: number;

  constructor(config: RateLimiterConfig) {
    this.capacity = config.capacity;
    this.refillRate = config.refillRate;
    this.tokens = config.capacity;
    this.lastRefill = now();
  }

  /**
   * Waiting for a token
   */
  async waitForToken(): Promise<void> {
    while (true) {
      this.refill();

      if (this.tokens >= 1) {
        this.tokens -= 1;
        return;
      }

      // Calculate Waiting Time
      const waitTime = this.calculateWaitTime();
      await this.sleep(waitTime);
    }
  }

  /**
   * Get the number of available tokens
   */
  getAvailableTokens(): number {
    this.refill();
    return this.tokens;
  }

  /**
   * Overlapping current limiters
   */
  reset(): void {
    this.tokens = this.capacity;
    this.lastRefill = now();
  }

  /**
   * filler token
   */
  private refill(): void {
    const currentTime = now();
    const timePassed = (currentTime - this.lastRefill) / 1000; // Convert to seconds
    const tokensToAdd = timePassed * this.refillRate;

    this.tokens = Math.min(this.capacity, this.tokens + tokensToAdd);
    this.lastRefill = currentTime;
  }

  /**
   * Calculate Waiting Time
   */
  private calculateWaitTime(): number {
    const tokensNeeded = 1;
    const tokensDeficit = tokensNeeded - this.tokens;
    const waitTime = (tokensDeficit / this.refillRate) * 1000; // Convert to milliseconds
    return Math.ceil(waitTime);
  }

  /**
   * delay function
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
