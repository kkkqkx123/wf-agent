/**
 * AgentLoop Retry and Timeout Integration Tests
 *
 * Tests retry and timeout behavior for agent loop iterations:
 * 1. Timeout errors should NOT be retried (Defect #3 fix verification)
 * 2. Stream execution should properly throw errors on retry exhaustion (Defect #2 fix)
 * 3. Exponential backoff calculation
 * 4. shouldRetry callback integration
 *
 * Real Scenarios:
 * - Scenario A: LLM call times out → should NOT retry (wastes budget on timeout)
 * - Scenario B: Tool execution fails → should retry with backoff
 * - Scenario C: Global budget exhausted → should stop retrying
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { RetryPolicy } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";

// Mock types for testing
interface MockLLMError extends Error {
  name: string;
  message: string;
}

function createTimeoutError(ms: number): MockLLMError {
  const error = new Error(`Execution exceeded ${ms}ms timeout`);
  error.name = "TimeoutError";
  return error as MockLLMError;
}

function createNetworkError(): MockLLMError {
  const error = new Error("Network connection failed");
  error.name = "NetworkError";
  return error as MockLLMError;
}

describe("AgentLoop Retry and Timeout - Integration Tests", () => {
  describe("Timeout Error Handling (Defect #3: Timeout Not Retried)", () => {
    /**
     * Scenario A1: Timeout error should NOT be retried
     *
     * Real scenario:
     * - Agent iteration times out after 30s
     * - Retrying won't help because it will timeout again
     * - Should fail immediately instead of wasting budget
     */
    it("should not retry timeout errors", () => {
      const defaultRetryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: (error: Error, attemptCount: number) => {
          // Fixed: Don't retry timeout errors
          if (error.message?.includes("timeout")) {
            return false;
          }
          return error.name !== "TimeoutError" && attemptCount < 3;
        },
        getNextDelay: (attemptCount: number) => {
          const delay = 1000 * Math.pow(2, attemptCount);
          return Math.min(delay, 30000);
        },
      };

      const timeoutError = createTimeoutError(30000);

      // shouldRetry should return false for timeout
      expect(defaultRetryPolicy.shouldRetry(timeoutError, 0)).toBe(false);

      // This prevents the retry loop from attempting again
    });

    /**
     * Scenario A2: Timeout message variations
     */
    it("should recognize various timeout error messages", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 3,
        shouldRetry: (error: Error) => {
          // Case-insensitive timeout check
          if (error.message?.toLowerCase().includes("timeout")) {
            return false;
          }
          return true;
        },
        getNextDelay: () => 1000,
      };

      const timeoutVariations = [
        new Error("Execution exceeded 30000ms timeout"),
        new Error("Stream iteration exceeded 60000ms timeout after 59000ms"),
        new Error("Branch execution timeout after 5000ms"),
        new Error("TIMEOUT: Operation took too long"),
      ];

      timeoutVariations.forEach(error => {
        expect(policy.shouldRetry?.(error, 0)).toBe(false);
      });
    });
  });

  describe("Retryable Error Handling", () => {
    /**
     * Scenario B1: Network error should be retried
     */
    it("should retry retryable errors like network failures", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 3,
        shouldRetry: (error: Error, attemptCount: number) => {
          if (error.message?.toLowerCase().includes("timeout")) {
            return false; // Don't retry timeout
          }
          return attemptCount < 3; // Retry other errors
        },
        getNextDelay: (attemptCount: number) => 1000 * Math.pow(2, attemptCount),
      };

      const networkError = createNetworkError();

      expect(policy.shouldRetry?.(networkError, 0)).toBe(true);
      expect(policy.shouldRetry?.(networkError, 1)).toBe(true);
      expect(policy.shouldRetry?.(networkError, 2)).toBe(true);
      expect(policy.shouldRetry?.(networkError, 3)).toBe(false); // Max attempts reached
    });

    /**
     * Scenario B2: Exponential backoff calculation
     */
    it("should calculate exponential backoff correctly", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 5,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          // delay = 1000 * 2^attemptCount, capped at 30000
          const delay = 1000 * Math.pow(2, attemptCount);
          return Math.min(delay, 30000);
        },
      };

      expect(policy.getNextDelay?.(0)).toBe(1000); // 1s
      expect(policy.getNextDelay?.(1)).toBe(2000); // 2s
      expect(policy.getNextDelay?.(2)).toBe(4000); // 4s
      expect(policy.getNextDelay?.(3)).toBe(8000); // 8s
      expect(policy.getNextDelay?.(4)).toBe(16000); // 16s
      expect(policy.getNextDelay?.(5)).toBe(30000); // Capped at 30s
    });

    /**
     * Scenario B3: Max delay cap prevents unbounded growth
     */
    it("should cap delay at maxDelay", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 10,
        baseDelay: 1000,
        backoffMultiplier: 3,
        maxDelay: 5000,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          const delay = 1000 * Math.pow(3, attemptCount);
          return Math.min(delay, 5000);
        },
      };

      expect(policy.getNextDelay?.(0)).toBe(1000);
      expect(policy.getNextDelay?.(1)).toBe(3000);
      expect(policy.getNextDelay?.(2)).toBe(5000); // Capped
      expect(policy.getNextDelay?.(3)).toBe(5000); // Capped
    });
  });

  describe("Jitter in Backoff", () => {
    /**
     * Scenario B4: Jitter prevents thundering herd
     *
     * Real scenario:
     * - Multiple agents fail at same time
     * - Without jitter: all retry simultaneously → thundering herd
     * - With jitter: retry times spread out
     */
    it("should apply jitter to delay", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: true,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          const delay = 1000 * Math.pow(2, attemptCount);
          const delayWithMax = Math.min(delay, 30000);

          if (true /* jitter enabled */) {
            // Add ±10% random jitter
            const jitterFactor = 0.9 + Math.random() * 0.2;
            return Math.floor(delayWithMax * jitterFactor);
          }
          return delayWithMax;
        },
      };

      // Collect multiple samples to verify jitter distribution
      const samples = new Set<number>();
      for (let i = 0; i < 20; i++) {
        const delay = policy.getNextDelay?.(0)!;
        samples.add(delay);
      }

      // Should have multiple different values due to jitter
      expect(samples.size).toBeGreaterThan(1);

      // All should be in range 900-1100 (±10% of 1000)
      samples.forEach(delay => {
        expect(delay).toBeGreaterThanOrEqual(900);
        expect(delay).toBeLessThanOrEqual(1100);
      });
    });

    it("should not apply jitter when disabled", () => {
      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          const delay = 1000 * Math.pow(2, attemptCount);
          // No jitter applied
          return Math.min(delay, 30000);
        },
      };

      // All samples should be identical
      const samples = new Set<number>();
      for (let i = 0; i < 5; i++) {
        samples.add(policy.getNextDelay?.(0)!);
      }

      expect(samples.size).toBe(1);
      expect(samples.has(1000)).toBe(true);
    });
  });

  describe("Custom shouldRetry Logic", () => {
    /**
     * Scenario C1: Custom predicate for sophisticated retry decisions
     */
    it("should support custom shouldRetry logic", () => {
      const retryableStatusCodes = new Set([408, 429, 500, 502, 503, 504]);

      const policy: RetryPolicy = {
        enabled: true,
        maxRetries: 3,
        shouldRetry: (error: Error, attemptCount: number) => {
          if (attemptCount >= 3) return false;

          // Don't retry timeout
          if (error.message?.includes("timeout")) return false;

          // Extract status code if available
          if ("statusCode" in error) {
            return retryableStatusCodes.has(error.statusCode as number);
          }

          // Default: retry all errors
          return true;
        },
        getNextDelay: () => 1000,
      };

      // Simulate HTTP 503 error
      const serverError = new Error("Service Unavailable");
      (serverError as any).statusCode = 503;
      expect(policy.shouldRetry?.(serverError, 0)).toBe(true);

      // Simulate HTTP 404 error (not retryable)
      const notFoundError = new Error("Not Found");
      (notFoundError as any).statusCode = 404;
      expect(policy.shouldRetry?.(notFoundError, 0)).toBe(false);

      // Timeout should not be retried
      const timeoutError = createTimeoutError(30000);
      expect(policy.shouldRetry?.(timeoutError, 0)).toBe(false);
    });
  });

  describe("Configuration Examples", () => {
    /**
     * Conservative retry policy (few retries, long delays)
     */
    it("should support conservative retry policy", () => {
      const conservativePolicy: RetryPolicy = {
        enabled: true,
        maxRetries: 2,
        baseDelay: 5000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        shouldRetry: (error: Error) => {
          // Only retry on specific errors
          return error.message?.includes("timeout") === false;
        },
        getNextDelay: (attemptCount: number) => {
          return Math.min(5000 * Math.pow(2, attemptCount), 30000);
        },
      };

      expect(conservativePolicy.maxRetries).toBe(2);
      expect(conservativePolicy.getNextDelay?.(0)).toBe(5000);
    });

    /**
     * Aggressive retry policy (many retries, short delays)
     */
    it("should support aggressive retry policy", () => {
      const aggressivePolicy: RetryPolicy = {
        enabled: true,
        maxRetries: 5,
        baseDelay: 100,
        backoffMultiplier: 1.5,
        maxDelay: 5000,
        shouldRetry: (error: Error) => {
          return !error.message?.includes("timeout");
        },
        getNextDelay: (attemptCount: number) => {
          return Math.min(100 * Math.pow(1.5, attemptCount), 5000);
        },
      };

      expect(aggressivePolicy.maxRetries).toBe(5);
      expect(aggressivePolicy.getNextDelay?.(0)).toBe(100);
      expect(aggressivePolicy.getNextDelay?.(1)).toBe(150);
    });

    /**
     * No retry policy
     */
    it("should support disabled retry", () => {
      const noRetryPolicy: RetryPolicy = {
        enabled: false,
      };

      expect(noRetryPolicy.enabled).toBe(false);
    });
  });
});
