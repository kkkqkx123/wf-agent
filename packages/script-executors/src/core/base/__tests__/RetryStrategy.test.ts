/**
 * RetryStrategy testing
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { RetryStrategy } from "../RetryStrategy.js";

describe("RetryStrategy", () => {
  let retryStrategy: RetryStrategy;

  beforeEach(() => {
    retryStrategy = new RetryStrategy();
  });

  describe("constructor", () => {
    it("Instances should be created using the default configuration", () => {
      const strategy = new RetryStrategy();
      expect(strategy).toBeInstanceOf(RetryStrategy);
    });

    it("Instances should be created using custom configurations", () => {
      const strategy = new RetryStrategy({
        maxRetries: 5,
        baseDelay: 2000,
        exponentialBackoff: false,
        maxDelay: 60000,
      });
      expect(strategy).toBeInstanceOf(RetryStrategy);
    });
  });

  describe("shouldRetry", () => {
    it("True should be returned when the maximum number of retries has not been reached", () => {
      const error = new Error("Test error");
      expect(retryStrategy.shouldRetry(error, 0)).toBe(true);
      expect(retryStrategy.shouldRetry(error, 1)).toBe(true);
      expect(retryStrategy.shouldRetry(error, 2)).toBe(true);
    });

    it("False should be returned when the maximum number of retries is reached", () => {
      const error = new Error("Test error");
      expect(retryStrategy.shouldRetry(error, 3)).toBe(false);
      expect(retryStrategy.shouldRetry(error, 4)).toBe(false);
    });

    it("ValidationError should not be retried", () => {
      const error = new Error("ValidationError: Invalid input");
      expect(retryStrategy.shouldRetry(error, 0)).toBe(false);
    });

    it("ConfigurationError should not be retried", () => {
      const error = new Error("ConfigurationError: Missing config");
      expect(retryStrategy.shouldRetry(error, 0)).toBe(false);
    });

    it("ScriptNotFoundError should not be retried", () => {
      const error = new Error("ScriptNotFoundError: File not found");
      expect(retryStrategy.shouldRetry(error, 0)).toBe(false);
    });

    it("Errors whose error name contains a non-retriable type should not be retried", () => {
      const error = new Error("Test");
      error.name = "ValidationError";
      expect(retryStrategy.shouldRetry(error, 0)).toBe(false);
    });

    it("Should be retried for common errors", () => {
      const error = new Error("Network timeout");
      expect(retryStrategy.shouldRetry(error, 0)).toBe(true);
    });
  });

  describe("getRetryDelay", () => {
    it("Exponential backoff should be used to calculate delays", () => {
      const strategy = new RetryStrategy({
        baseDelay: 1000,
        exponentialBackoff: true,
        maxDelay: 30000,
      });

      expect(strategy.getRetryDelay(0)).toBe(1000);
      expect(strategy.getRetryDelay(1)).toBe(2000);
      expect(strategy.getRetryDelay(2)).toBe(4000);
      expect(strategy.getRetryDelay(3)).toBe(8000);
    });

    it("Fixed delay should be used", () => {
      const strategy = new RetryStrategy({
        baseDelay: 2000,
        exponentialBackoff: false,
      });

      expect(strategy.getRetryDelay(0)).toBe(2000);
      expect(strategy.getRetryDelay(1)).toBe(2000);
      expect(strategy.getRetryDelay(2)).toBe(2000);
    });

    it("Maximum delay should be limited", () => {
      const strategy = new RetryStrategy({
        baseDelay: 1000,
        exponentialBackoff: true,
        maxDelay: 5000,
      });

      expect(strategy.getRetryDelay(0)).toBe(1000);
      expect(strategy.getRetryDelay(1)).toBe(2000);
      expect(strategy.getRetryDelay(2)).toBe(4000);
      expect(strategy.getRetryDelay(3)).toBe(5000); // Limited to maxDelay
      expect(strategy.getRetryDelay(4)).toBe(5000);
    });
  });

  describe("createDefault", () => {
    it("Default retry policy instances should be created", () => {
      const defaultStrategy = RetryStrategy.createDefault();
      expect(defaultStrategy).toBeInstanceOf(RetryStrategy);
    });
  });
});
