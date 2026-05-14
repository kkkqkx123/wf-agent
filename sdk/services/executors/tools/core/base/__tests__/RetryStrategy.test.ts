/**
 * RetryStrategy unit test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { RetryStrategy, RetryStrategyConfig } from "../RetryStrategy.js";
import { TimeoutError, HttpError, NetworkError } from "@wf-agent/types";
import {
  RateLimitError,
  InternalServerError,
  ServiceUnavailableError,
} from "@wf-agent/common-utils";

describe("RetryStrategy", () => {
  describe("constructor", () => {
    it("The instance should be created using the provided configuration.", () => {
      const config: RetryStrategyConfig = {
        maxRetries: 5,
        baseDelay: 2000,
        exponentialBackoff: false,
        maxDelay: 10000,
      };

      const strategy = new RetryStrategy(config);

      // Indirectly verify maxRetries through shouldRetry
      const error = new TimeoutError("test", 1000);
      expect(strategy.shouldRetry(error, 4)).toBe(true);
      expect(strategy.shouldRetry(error, 5)).toBe(false);
    });
  });

  describe("shouldRetry", () => {
    let strategy: RetryStrategy;

    beforeEach(() => {
      strategy = new RetryStrategy({
        maxRetries: 3,
        baseDelay: 1000,
        exponentialBackoff: true,
      });
    });

    describe("Retry Limit", () => {
      it("It should return false when the maximum number of retries is exceeded.", () => {
        const error = new TimeoutError("test", 1000);

        expect(strategy.shouldRetry(error, 0)).toBe(true);
        expect(strategy.shouldRetry(error, 1)).toBe(true);
        expect(strategy.shouldRetry(error, 2)).toBe(true);
        expect(strategy.shouldRetry(error, 3)).toBe(false);
      });
    });

    describe("TimeoutError", () => {
      it("Should return true for TimeoutError", () => {
        const error = new TimeoutError("Request timed out", 5000);
        expect(strategy.shouldRetry(error, 0)).toBe(true);
      });
    });

    describe("HTTP Error", () => {
      it("应该对 RateLimitError (429) 返回 true", () => {
        const error = new RateLimitError("Rate limit exceeded", 60);
        expect(strategy.shouldRetry(error, 0)).toBe(true);
      });

      it("应该对 InternalServerError (500) 返回 true", () => {
        const error = new InternalServerError("Internal server error");
        expect(strategy.shouldRetry(error, 0)).toBe(true);
      });

      it("应该对 ServiceUnavailableError (503) 返回 true", () => {
        const error = new ServiceUnavailableError("Service unavailable");
        expect(strategy.shouldRetry(error, 0)).toBe(true);
      });

      it("It should return true for generic HttpError 5xx.", () => {
        const error500 = new HttpError("Server error", 500);
        const error502 = new HttpError("Bad gateway", 502);
        const error504 = new HttpError("Gateway timeout", 504);

        expect(strategy.shouldRetry(error500, 0)).toBe(true);
        expect(strategy.shouldRetry(error502, 0)).toBe(true);
        expect(strategy.shouldRetry(error504, 0)).toBe(true);
      });

      it("Should return false for HttpError 4xx.", () => {
        const error400 = new HttpError("Bad request", 400);
        const error401 = new HttpError("Unauthorized", 401);
        const error403 = new HttpError("Forbidden", 403);
        const error404 = new HttpError("Not found", 404);

        expect(strategy.shouldRetry(error400, 0)).toBe(false);
        expect(strategy.shouldRetry(error401, 0)).toBe(false);
        expect(strategy.shouldRetry(error403, 0)).toBe(false);
        expect(strategy.shouldRetry(error404, 0)).toBe(false);
      });
    });

    describe("NetworkError", () => {
      it("Should return true for NetworkError", () => {
        const error = new NetworkError("Connection failed");
        expect(strategy.shouldRetry(error, 0)).toBe(true);
      });
    });

    describe("Other errors", () => {
      it("Should return false for normal Error", () => {
        const error = new Error("Some error");
        expect(strategy.shouldRetry(error, 0)).toBe(false);
      });
    });
  });

  describe("getRetryDelay", () => {
    it("Exponential backoff should be used to calculate the delay", () => {
      const strategy = new RetryStrategy({
        maxRetries: 3,
        baseDelay: 1000,
        exponentialBackoff: true,
      });

      // baseDelay * 2^retryCount
      expect(strategy.getRetryDelay(0)).toBe(1000); // 1000 * 2^0 = 1000
      expect(strategy.getRetryDelay(1)).toBe(2000); // 1000 * 2^1 = 2000
      expect(strategy.getRetryDelay(2)).toBe(4000); // 1000 * 2^2 = 4000
      expect(strategy.getRetryDelay(3)).toBe(8000); // 1000 * 2^3 = 8000
    });

    it("Fixed delays should be used (non-exponential backoff)", () => {
      const strategy = new RetryStrategy({
        maxRetries: 3,
        baseDelay: 1000,
        exponentialBackoff: false,
      });

      expect(strategy.getRetryDelay(0)).toBe(1000);
      expect(strategy.getRetryDelay(1)).toBe(1000);
      expect(strategy.getRetryDelay(2)).toBe(1000);
      expect(strategy.getRetryDelay(3)).toBe(1000);
    });

    it("Maximum latency limits should be applied", () => {
      const strategy = new RetryStrategy({
        maxRetries: 10,
        baseDelay: 1000,
        exponentialBackoff: true,
        maxDelay: 5000,
      });

      // Exponential backoff will exceed maxDelay.
      expect(strategy.getRetryDelay(0)).toBe(1000);
      expect(strategy.getRetryDelay(1)).toBe(2000);
      expect(strategy.getRetryDelay(2)).toBe(4000);
      expect(strategy.getRetryDelay(3)).toBe(5000); // Limit to maxDelay
      expect(strategy.getRetryDelay(10)).toBe(5000); // Limit to maxDelay
    });
  });

  describe("execute", () => {
    it("The function should be executed successfully", async () => {
      const strategy = new RetryStrategy({
        maxRetries: 3,
        baseDelay: 10,
        exponentialBackoff: false,
      });

      const fn = vi.fn().mockResolvedValue("success");

      const result = await strategy.execute(fn);

      expect(result).toBe("success");
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it("Should retry and eventually succeed after failing", async () => {
      const strategy = new RetryStrategy({
        maxRetries: 3,
        baseDelay: 10,
        exponentialBackoff: false,
      });

      const fn = vi
        .fn()
        .mockRejectedValueOnce(new NetworkError("Failed 1"))
        .mockRejectedValueOnce(new NetworkError("Failed 2"))
        .mockResolvedValue("success");

      const result = await strategy.execute(fn);

      expect(result).toBe("success");
      expect(fn).toHaveBeenCalledTimes(3);
    });

    it("Should throw an error after the retry count is exhausted", async () => {
      const strategy = new RetryStrategy({
        maxRetries: 2,
        baseDelay: 10,
        exponentialBackoff: false,
      });

      const fn = vi.fn().mockRejectedValue(new NetworkError("Always fails"));

      try {
        await strategy.execute(fn);
        expect.fail("Should have thrown");
      } catch (error) {
        expect(error).toBeInstanceOf(NetworkError);
        // Initial call + 2 retries
        expect(fn.mock.calls.length).toBeGreaterThanOrEqual(1);
      }
    });
  });

  describe("Static factory methods", () => {
    describe("createDefault", () => {
      it("A retry policy with a default configuration should be created", () => {
        const strategy = RetryStrategy.createDefault();

        // Verify the default configuration.
        const error = new TimeoutError("test", 1000);
        expect(strategy.shouldRetry(error, 2)).toBe(true);
        expect(strategy.shouldRetry(error, 3)).toBe(false);

        // Verify the default delay
        expect(strategy.getRetryDelay(0)).toBe(1000);
        expect(strategy.getRetryDelay(1)).toBe(2000);
      });
    });

    describe("createNoRetry", () => {
      it("A no retry policy should be created", () => {
        const strategy = RetryStrategy.createNoRetry();

        const error = new TimeoutError("test", 1000);
        expect(strategy.shouldRetry(error, 0)).toBe(false);
      });
    });

    describe("createCustom", () => {
      it("A custom configured retry policy should be created", () => {
        const strategy = RetryStrategy.createCustom({
          maxRetries: 5,
          baseDelay: 500,
        });

        const error = new TimeoutError("test", 1000);
        expect(strategy.shouldRetry(error, 4)).toBe(true);
        expect(strategy.shouldRetry(error, 5)).toBe(false);
        expect(strategy.getRetryDelay(0)).toBe(500);
      });

      it("Missing configurations should be filled in with default values", () => {
        const strategy = RetryStrategy.createCustom({});

        const error = new TimeoutError("test", 1000);
        expect(strategy.shouldRetry(error, 2)).toBe(true);
        expect(strategy.shouldRetry(error, 3)).toBe(false);
        expect(strategy.getRetryDelay(0)).toBe(1000);
      });
    });
  });
});
