/**
 * Abort Signal Utils Unit Tests
 * Tests for abort-signal-utils.ts
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  checkInterruption,
  shouldContinue,
  isInterrupted,
  getInterruptionDescription,
  withInterruptionCheck,
  withInterruptionCheckIter,
} from "../../src/utils/signal/abort-signal-utils.js";

describe("Abort Signal Utils", () => {
  describe("checkInterruption", () => {
    it("should return continue when signal is undefined", () => {
      const result = checkInterruption(undefined);
      expect(result).toEqual({ type: "continue" });
    });

    it("should return continue when signal is not aborted", () => {
      const controller = new AbortController();
      const result = checkInterruption(controller.signal);
      expect(result).toEqual({ type: "continue" });
    });

    it("should return aborted when signal is aborted without reason", () => {
      const controller = new AbortController();
      controller.abort();
      const result = checkInterruption(controller.signal);
      expect(result.type).toBe("aborted");
      if (result.type === "aborted") {
        expect(result.reason).toBeDefined();
      }
    });

    it("should return aborted with custom reason", () => {
      const controller = new AbortController();
      const customReason = new Error("Custom abort reason");
      controller.abort(customReason);
      const result = checkInterruption(controller.signal);
      expect(result.type).toBe("aborted");
      if (result.type === "aborted") {
        expect(result.reason).toBe(customReason);
      }
    });

    it("should return aborted with string reason", () => {
      const controller = new AbortController();
      controller.abort("Operation cancelled");
      const result = checkInterruption(controller.signal);
      expect(result.type).toBe("aborted");
      if (result.type === "aborted") {
        expect(result.reason).toBe("Operation cancelled");
      }
    });
  });

  describe("shouldContinue", () => {
    it("should return true for continue result", () => {
      expect(shouldContinue({ type: "continue" })).toBe(true);
    });

    it("should return false for aborted result", () => {
      expect(shouldContinue({ type: "aborted" })).toBe(false);
    });

    it("should return false for aborted result with reason", () => {
      expect(shouldContinue({ type: "aborted", reason: "test" })).toBe(false);
    });
  });

  describe("isInterrupted", () => {
    it("should return false for continue result", () => {
      expect(isInterrupted({ type: "continue" })).toBe(false);
    });

    it("should return true for aborted result", () => {
      expect(isInterrupted({ type: "aborted" })).toBe(true);
    });

    it("should return true for aborted result with reason", () => {
      expect(isInterrupted({ type: "aborted", reason: "test" })).toBe(true);
    });

    it("should provide type narrowing for aborted results", () => {
      const result: ReturnType<typeof checkInterruption> = { type: "aborted", reason: "test" };
      
      if (isInterrupted(result)) {
        // TypeScript should know result is { type: "aborted"; reason?: unknown }
        expect(result.type).toBe("aborted");
        expect(result.reason).toBe("test");
      }
    });
  });

  describe("getInterruptionDescription", () => {
    it("should return 'Execution continuing' for continue result", () => {
      expect(getInterruptionDescription({ type: "continue" })).toBe("Execution continuing");
    });

    it("should return 'Operation aborted' for aborted result without reason", () => {
      expect(getInterruptionDescription({ type: "aborted" })).toBe("Operation aborted");
    });

    it("should return reason string for aborted result with reason", () => {
      expect(getInterruptionDescription({ type: "aborted", reason: "Custom reason" })).toBe("Custom reason");
    });

    it("should convert error reason to string", () => {
      const error = new Error("Test error");
      expect(getInterruptionDescription({ type: "aborted", reason: error })).toBe("Error: Test error");
    });

    it("should handle number reason", () => {
      expect(getInterruptionDescription({ type: "aborted", reason: 42 })).toBe("42");
    });
  });

  describe("withInterruptionCheck", () => {
    it("should execute function and return completed status when not interrupted", async () => {
      const fn = async (signal: AbortSignal) => "result";
      const result = await withInterruptionCheck(fn);
      
      expect(result.status).toBe("completed");
      if (result.status === "completed") {
        expect(result.result).toBe("result");
      }
    });

    it("should return interrupted status when signal is already aborted", async () => {
      const controller = new AbortController();
      controller.abort("Pre-aborted");
      
      const fn = async (signal: AbortSignal) => "result";
      const result = await withInterruptionCheck(fn, controller.signal);
      
      expect(result.status).toBe("interrupted");
      if (result.status === "interrupted") {
        expect(result.interruption.type).toBe("aborted");
      }
    });

    it("should pass signal to function for proper cancellation support", async () => {
      const controller = new AbortController();
      let receivedSignal: AbortSignal | undefined;
      
      const fn = async (signal: AbortSignal) => {
        receivedSignal = signal;
        return "result";
      };
      
      await withInterruptionCheck(fn, controller.signal);
      
      expect(receivedSignal).toBe(controller.signal);
    });

    it("should handle AbortError thrown by function with signal", async () => {
      const controller = new AbortController();
      
      const fn = async (signal: AbortSignal) => {
        // Simulate checking signal and throwing
        if (signal.aborted) {
          throw new DOMException("Aborted", "AbortError");
        }
        return "result";
      };
      
      controller.abort("Aborted during execution");
      const result = await withInterruptionCheck(fn, controller.signal);
      
      expect(result.status).toBe("interrupted");
      if (result.status === "interrupted") {
        expect(result.interruption.type).toBe("aborted");
      }
    });

    it("should handle Error with AbortError name", async () => {
      const controller = new AbortController();
      
      const fn = async (signal: AbortSignal) => {
        const error = new Error("Aborted");
        error.name = "AbortError";
        throw error;
      };
      
      controller.abort("Aborted with Error");
      const result = await withInterruptionCheck(fn, controller.signal);
      
      expect(result.status).toBe("interrupted");
    });

    it("should not catch AbortError when signal is not aborted", async () => {
      const fn = async (signal: AbortSignal) => {
        const error = new Error("Aborted");
        error.name = "AbortError";
        throw error;
      };
      
      // Signal is not aborted, so the error should be rethrown
      await expect(withInterruptionCheck(fn)).rejects.toThrow("Aborted");
    });

    it("should rethrow non-AbortError exceptions", async () => {
      const fn = async (signal: AbortSignal) => {
        throw new Error("Regular error");
      };
      
      await expect(withInterruptionCheck(fn)).rejects.toThrow("Regular error");
    });

    it("should work without signal parameter", async () => {
      const fn = async (signal: AbortSignal) => 42;
      const result = await withInterruptionCheck(fn);
      
      expect(result.status).toBe("completed");
      if (result.status === "completed") {
        expect(result.result).toBe(42);
      }
    });

    it("should check signal after function completes", async () => {
      const controller = new AbortController();
      
      const fn = async (signal: AbortSignal) => {
        // Simulate work that respects signal
        await new Promise((resolve, reject) => {
          const timer = setTimeout(resolve, 10);
          signal.addEventListener('abort', () => {
            clearTimeout(timer);
            reject(new DOMException("Aborted", "AbortError"));
          });
        });
        return "result";
      };
      
      // Abort during execution
      setTimeout(() => controller.abort("Aborted during execution"), 5);
      
      const result = await withInterruptionCheck(fn, controller.signal);
      
      // Should detect the abort
      expect(result.status).toBe("interrupted");
    });
  });

  describe("withInterruptionCheckIter", () => {
    it("should iterate through all values when not interrupted", async () => {
      async function* generator() {
        yield 1;
        yield 2;
        yield 3;
      }
      
      const results: (number | ReturnType<typeof checkInterruption>)[] = [];
      for await (const value of withInterruptionCheckIter(generator())) {
        results.push(value);
      }
      
      expect(results).toEqual([1, 2, 3]);
    });

    it("should stop iteration when signal is aborted before starting", async () => {
      const controller = new AbortController();
      controller.abort("Stopped before start");
      
      async function* generator() {
        yield 1;
        yield 2;
        yield 3;
      }
      
      const results: (number | ReturnType<typeof checkInterruption>)[] = [];
      for await (const value of withInterruptionCheckIter(generator(), controller.signal)) {
        results.push(value);
      }
      
      expect(results.length).toBe(1);
      expect(results[0]).toEqual({ type: "aborted", reason: "Stopped before start" });
    });

    it("should stop iteration when signal is aborted during iteration", async () => {
      const controller = new AbortController();
      
      async function* generator() {
        yield 1;
        yield 2;
        controller.abort("Stopped mid-iteration");
        yield 3;
      }
      
      const results: (number | ReturnType<typeof checkInterruption>)[] = [];
      for await (const value of withInterruptionCheckIter(generator(), controller.signal)) {
        results.push(value);
      }
      
      // Should have yielded 1, 2, then the interruption (the 3 won't be yielded because we check before yielding)
      expect(results.length).toBeGreaterThanOrEqual(3);
      expect(results[0]).toBe(1);
      expect(results[1]).toBe(2);
      // The last item should be an interruption
      const lastItem = results[results.length - 1];
      expect(lastItem).toHaveProperty("type", "aborted");
    });

    it("should clean up iterator on interruption", async () => {
      const controller = new AbortController();
      let returnCalled = false;
      
      const mockIterator = {
        async next() {
          if (!controller.signal.aborted) {
            return { value: 1, done: false };
          }
          return { value: undefined, done: true };
        },
        async return() {
          returnCalled = true;
          return { value: undefined, done: true };
        },
        [Symbol.asyncIterator]() {
          return this;
        },
      };
      
      controller.abort();
      
      const results: unknown[] = [];
      for await (const value of withInterruptionCheckIter(mockIterator as any, controller.signal)) {
        results.push(value);
      }
      
      expect(returnCalled).toBe(true);
      expect(results.length).toBe(1);
      // The result should be an aborted interruption (with or without reason)
      const firstResult = results[0] as ReturnType<typeof checkInterruption>;
      expect(firstResult.type).toBe("aborted");
    });

    it("should handle empty iterable", async () => {
      async function* generator() {
        // Empty generator
      }
      
      const results: (number | ReturnType<typeof checkInterruption>)[] = [];
      for await (const value of withInterruptionCheckIter(generator())) {
        results.push(value);
      }
      
      expect(results).toEqual([]);
    });

    it("should work without signal parameter", async () => {
      async function* generator() {
        yield "a";
        yield "b";
        yield "c";
      }
      
      const results: (string | ReturnType<typeof checkInterruption>)[] = [];
      for await (const value of withInterruptionCheckIter(generator())) {
        results.push(value);
      }
      
      expect(results).toEqual(["a", "b", "c"]);
    });

    it("should handle object values", async () => {
      async function* generator() {
        yield { id: 1, name: "first" };
        yield { id: 2, name: "second" };
      }
      
      const results: unknown[] = [];
      for await (const value of withInterruptionCheckIter(generator())) {
        results.push(value);
      }
      
      expect(results).toEqual([
        { id: 1, name: "first" },
        { id: 2, name: "second" },
      ]);
    });
  });
});
