import { describe, it, expect, beforeEach } from "vitest";
import { RateLimiter } from "../rate-limiter.js";

describe("RateLimiter", () => {
  let rateLimiter: RateLimiter;

  beforeEach(() => {
    rateLimiter = new RateLimiter({ capacity: 10, refillRate: 5 });
  });

  describe("constructor", () => {
    it("should initialize with full tokens", () => {
      expect(rateLimiter.getAvailableTokens()).toBe(10);
    });

    it("should allow zero capacity", () => {
      const rl = new RateLimiter({ capacity: 0, refillRate: 1 });
      expect(rl.getAvailableTokens()).toBe(0);
    });
  });

  describe("waitForToken", () => {
    it("should consume a token immediately when available", async () => {
      const before = rateLimiter.getAvailableTokens();
      await rateLimiter.waitForToken();
      const after = rateLimiter.getAvailableTokens();
      expect(after).toBe(before - 1);
    });

    it("should consume all tokens then wait", async () => {
      // Consume all tokens first (10 tokens)
      for (let i = 0; i < 10; i++) {
        await rateLimiter.waitForToken();
      }
      expect(rateLimiter.getAvailableTokens()).toBeCloseTo(0, 2);

      // Next call should eventually get a token after refill
      const waitPromise = rateLimiter.waitForToken();
      // Should resolve eventually (refill rate is 5/sec, so ~200ms for 1 token at 0 tokens)
      await expect(waitPromise).resolves.toBeUndefined();
    }, 2000);
  });

  describe("getAvailableTokens", () => {
    it("should return initial capacity", () => {
      expect(rateLimiter.getAvailableTokens()).toBe(10);
    });

    it("should reflect consumed tokens", async () => {
      await rateLimiter.waitForToken();
      expect(rateLimiter.getAvailableTokens()).toBe(9);
    });

    it("should refill over time", async () => {
      await rateLimiter.waitForToken(); // consume 1, 9 left
      await rateLimiter.waitForToken(); // consume 1, 8 left
      // Now tokens = 8, wait for refill
      await new Promise(resolve => setTimeout(resolve, 300)); // ~1.5 tokens refilled
      const tokens = rateLimiter.getAvailableTokens();
      expect(tokens).toBeGreaterThanOrEqual(9);
      expect(tokens).toBeLessThanOrEqual(10);
    }, 2000);
  });

  describe("reset", () => {
    it("should reset to full capacity", async () => {
      await rateLimiter.waitForToken();
      await rateLimiter.waitForToken();
      expect(rateLimiter.getAvailableTokens()).toBeLessThan(10);

      rateLimiter.reset();
      expect(rateLimiter.getAvailableTokens()).toBe(10);
    });
  });

  describe("edge cases", () => {
    it("should not exceed capacity on refill", async () => {
      await rateLimiter.waitForToken(); // 9 tokens, 1 used
      // Simulate immediate refill (should cap at capacity)
      await new Promise(resolve => setTimeout(resolve, 100));
      const tokens = rateLimiter.getAvailableTokens();
      expect(tokens).toBeLessThanOrEqual(10);
    });
  });
});
