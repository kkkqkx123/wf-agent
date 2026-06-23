/**
 * Unit Tests for Timeout Utilities
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  combineTimeoutWithSignal,
  createTimeoutPromise,
  calculateAdaptiveTimeout,
  delay,
  withTimeout,
  isTimeoutError,
  createTimeoutError,
  executeWithSharedTimeout,
} from "../timeout-utils.js";

describe("calculateAdaptiveTimeout", () => {
  it("should return base timeout for retryCount 0", () => {
    expect(calculateAdaptiveTimeout(5000, 0, 30000)).toBe(5000);
  });

  it("should double for each retry count", () => {
    expect(calculateAdaptiveTimeout(1000, 1, 10000)).toBe(2000);
    expect(calculateAdaptiveTimeout(1000, 2, 10000)).toBe(4000);
    expect(calculateAdaptiveTimeout(1000, 3, 10000)).toBe(8000);
  });

  it("should cap at maxTimeout", () => {
    expect(calculateAdaptiveTimeout(1000, 5, 10000)).toBe(10000);
  });

  it("should work with zero base timeout", () => {
    expect(calculateAdaptiveTimeout(0, 0, 10000)).toBe(0);
    expect(calculateAdaptiveTimeout(0, 5, 10000)).toBe(0);
  });

  it("should handle high retry count correctly", () => {
    const result = calculateAdaptiveTimeout(100, 100, 50000);
    expect(result).toBe(50000);
  });
});

describe("isTimeoutError", () => {
  it("should return false for non-error input", () => {
    expect(isTimeoutError(null)).toBe(false);
    expect(isTimeoutError(undefined)).toBe(false);
    expect(isTimeoutError("timeout")).toBe(false);
    expect(isTimeoutError(42)).toBe(false);
  });

  it("should detect errors with 'timed out' in message", () => {
    const error = new Error("Operation timed out after 5000ms");
    expect(isTimeoutError(error)).toBe(true);
  });

  it("should detect errors with 'timeout' in message", () => {
    const error = new Error("timeout exceeded");
    expect(isTimeoutError(error)).toBe(true);
  });

  it("should detect TimeoutError name", () => {
    const error = new Error("some error");
    error.name = "TimeoutError";
    expect(isTimeoutError(error)).toBe(true);
  });

  it("should return false for unrelated errors", () => {
    const error = new Error("connection refused");
    expect(isTimeoutError(error)).toBe(false);
  });

  it("should return false for TypeError", () => {
    const error = new TypeError("invalid type");
    expect(isTimeoutError(error)).toBe(false);
  });
});

describe("createTimeoutError", () => {
  it("should create an error with the correct message format", () => {
    const error = createTimeoutError("llm-call-001", 5000, 5023);
    expect(error.message).toBe("Timeout 'llm-call-001' expired after 5023ms (configured: 5000ms)");
    expect(error.name).toBe("TimeoutError");
  });

  it("should include tag info when provided", () => {
    const error = createTimeoutError("llm-call-001", 5000, 5023, "llm-call");
    expect(error.message).toContain("tag: llm-call");
    expect(error.name).toBe("TimeoutError");
  });

  it("should handle zero durations", () => {
    const error = createTimeoutError("test", 0, 0);
    expect(error.message).toContain("configured: 0ms");
  });

  it("should handle empty timeout ID", () => {
    const error = createTimeoutError("", 1000, 1000);
    expect(error.message).toContain("Timeout '' expired after");
  });
});

describe("combineTimeoutWithSignal", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should create a timeout-only signal when no signal provided", () => {
    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(10000);
    expect(signal.aborted).toBe(false);

    cleanup();
  });

  it("should abort after duration when no signal provided", () => {
    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(5000);

    expect(signal.aborted).toBe(false);

    vi.advanceTimersByTime(5000);
    expect(signal.aborted).toBe(true);

    cleanup();
  });

  it("should return original signal and no-op cleanup when signal already aborted", () => {
    const controller = new AbortController();
    controller.abort("test reason");

    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(5000, controller.signal);
    expect(signal.aborted).toBe(true);
    expect(signal.reason).toBe("test reason");

    // Should not throw
    cleanup();
  });

  it("should abort when the combined signal aborts", () => {
    const controller = new AbortController();
    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(5000, controller.signal);

    expect(signal.aborted).toBe(false);

    controller.abort("user cancelled");
    expect(signal.aborted).toBe(true);

    cleanup();
  });

  it("should not fire timeout after cleanup", () => {
    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(5000);

    cleanup();
    vi.advanceTimersByTime(5000);
    expect(signal.aborted).toBe(false);
  });

  it("should not propagate abort after cleanup", () => {
    const controller = new AbortController();
    const { signal, clearTimeout: cleanup } = combineTimeoutWithSignal(5000, controller.signal);

    cleanup();
    controller.abort("should not propagate");
    expect(signal.aborted).toBe(false);
  });
});

describe("createTimeoutPromise", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should resolve with the original promise result", async () => {
    const promise = Promise.resolve("success");
    const result = await createTimeoutPromise(promise, 5000);
    expect(result).toBe("success");
  });

  it("should reject with timeout error if promise takes too long", async () => {
    const slowPromise = new Promise<string>(resolve => {
      setTimeout(() => resolve("too late"), 10000);
    });

    const resultPromise = createTimeoutPromise(slowPromise, 5000, "Custom timeout message");

    vi.advanceTimersByTime(5000);

    await expect(resultPromise).rejects.toThrow("Custom timeout message");
  });

  it("should reject with default message when no custom message provided", async () => {
    const slowPromise = new Promise<string>(() => {
      // Never resolves
    });

    const resultPromise = createTimeoutPromise(slowPromise, 3000);

    vi.advanceTimersByTime(3000);

    await expect(resultPromise).rejects.toThrow("Operation timed out after 3000ms");
  });

  it("should reject with original promise error", async () => {
    const failingPromise = Promise.reject(new Error("original error"));
    const resultPromise = createTimeoutPromise(failingPromise, 5000);

    await expect(resultPromise).rejects.toThrow("original error");
  });
});

describe("delay", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should resolve after the specified delay", async () => {
    const delayPromise = delay(1000);

    vi.advanceTimersByTime(1000);
    await expect(delayPromise).resolves.toBeUndefined();
  });

  it("should reject immediately if signal is already aborted", async () => {
    const controller = new AbortController();
    const reason = new Error("cancelled");
    controller.abort(reason);

    const delayPromise = delay(1000, controller.signal);

    await expect(delayPromise).rejects.toThrow("cancelled");
  });

  it("should reject with default reason if signal aborted with no reason", async () => {
    const controller = new AbortController();
    controller.abort();

    const delayPromise = delay(1000, controller.signal);

    // When abort() is called with no args, signal.reason is a DOMException-like
    // object coercing to "This operation was aborted". The fallback
    // "Delay aborted" is only used when signal.reason is falsy.
    await expect(delayPromise).rejects.toThrow();
  });

  it("should reject when signal aborts during delay", async () => {
    const controller = new AbortController();
    const delayPromise = delay(5000, controller.signal);

    controller.abort(new Error("stopped"));

    await expect(delayPromise).rejects.toThrow("stopped");
  });

  it("should resolve if signal does not abort before delay completes", async () => {
    const controller = new AbortController();
    const delayPromise = delay(1000, controller.signal);

    vi.advanceTimersByTime(1000);
    await expect(delayPromise).resolves.toBeUndefined();
  });
});

describe("withTimeout", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return the result of the function", async () => {
    const fn = async () => "result";
    const result = await withTimeout(fn, 5000);
    expect(result).toBe("result");
  });

  it("should reject with timeout error if function takes too long", async () => {
    const slowFn = async () => {
      await delay(10000);
      return "late";
    };

    const resultPromise = withTimeout(slowFn, 5000, {
      message: "custom timeout",
    });

    vi.advanceTimersByTime(5000);

    await expect(resultPromise).rejects.toThrow("custom timeout");
  });

  it("should call onTimeout callback when timeout occurs", async () => {
    const onTimeout = vi.fn();
    const neverResolving = async () => new Promise(() => {});

    const resultPromise = withTimeout(neverResolving, 1000, { onTimeout });

    vi.advanceTimersByTime(1000);

    await expect(resultPromise).rejects.toThrow();
    expect(onTimeout).toHaveBeenCalledTimes(1);
  });
});

describe("executeWithSharedTimeout", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return results for all operations", async () => {
    const operations = {
      a: async () => "result-a",
      b: async () => "result-b",
    };

    const results = await executeWithSharedTimeout(operations, 5000);
    expect(results.get("a")).toBe("result-a");
    expect(results.get("b")).toBe("result-b");
    expect(results.size).toBe(2);
  });

  it("should reject if operations exceed timeout", async () => {
    const operations = {
      slow: async () => {
        await delay(10000);
        return "too late";
      },
    };

    const resultPromise = executeWithSharedTimeout(operations, 1000, {
      message: "batch timeout",
    });

    vi.advanceTimersByTime(1000);

    await expect(resultPromise).rejects.toThrow("batch timeout");
  });

  it("should reject if any operation fails", async () => {
    const operations = {
      good: async () => "ok",
      bad: async () => {
        throw new Error("operation failed");
      },
    };

    await expect(executeWithSharedTimeout(operations, 5000)).rejects.toThrow("operation failed");
  });

  it("should call onTimeout callback when timeout occurs", async () => {
    const onTimeout = vi.fn();
    const operations = {
      slow: async () => {
        await delay(10000);
        return "late";
      },
    };

    const resultPromise = executeWithSharedTimeout(operations, 1000, { onTimeout });

    vi.advanceTimersByTime(1000);

    await expect(resultPromise).rejects.toThrow();
    expect(onTimeout).toHaveBeenCalledTimes(1);
  });

  it("should handle empty operations map", async () => {
    const results = await executeWithSharedTimeout({}, 5000);
    expect(results.size).toBe(0);
  });
});
