/**
 * Unit Tests for Unified Interruption Handler
 */

import { describe, it, expect, vi } from "vitest";
import {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "../interruption-handler.js";

describe("executeWithInterruptionHandling", () => {
  it("should return success with result when operation completes", async () => {
    const result = await executeWithInterruptionHandling(async () => "done");

    expect(result).toEqual({ success: true, result: "done" });
  });

  it("should pass signal to operation", async () => {
    const operation = vi.fn().mockResolvedValue("ok");
    const controller = new AbortController();

    await executeWithInterruptionHandling(operation, controller.signal);

    expect(operation).toHaveBeenCalledWith(controller.signal);
  });

  it("should detect pre-execution abort (PAUSE)", async () => {
    const controller = new AbortController();
    const reason = new Error("Execution paused") as Error & {
      interruptionType: string;
    };
    reason.interruptionType = "PAUSE";
    controller.abort(reason);

    const result = await executeWithInterruptionHandling(async () => "never", controller.signal);

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.interruption.type).toBe("paused");
    }
  });

  it("should detect pre-execution abort (STOP)", async () => {
    const controller = new AbortController();
    const reason = new Error("Execution stopped") as Error & {
      interruptionType: string;
    };
    reason.interruptionType = "STOP";
    controller.abort(reason);

    const result = await executeWithInterruptionHandling(async () => "never", controller.signal);

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.interruption.type).toBe("stopped");
    }
  });

  it("should detect post-execution abort", async () => {
    const controller = new AbortController();

    const result = await executeWithInterruptionHandling(async () => {
      const reason = new Error("Execution paused") as Error & {
        interruptionType: string;
      };
      reason.interruptionType = "PAUSE";
      controller.abort(reason);
      return "too-late";
    }, controller.signal);

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.interruption.type).toBe("paused");
    }
  });

  it("should re-throw non-abort errors", async () => {
    const controller = new AbortController();

    await expect(
      executeWithInterruptionHandling(
        async () => {
          throw new Error("unexpected error");
        },
        controller.signal,
      ),
    ).rejects.toThrow("unexpected error");
  });

  it("should handle InterruptionError with interruption property", async () => {
    const controller = new AbortController();

    const result = await executeWithInterruptionHandling(async () => {
      const error = new Error("Interruption") as Error & {
        name: string;
        interruption: { type: "paused" };
      };
      error.name = "InterruptionError";
      error.interruption = { type: "paused" };
      throw error;
    }, controller.signal);

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.interruption.type).toBe("paused");
    }
  });

  it("should handle AbortError from operation", async () => {
    const controller = new AbortController();

    const result = await executeWithInterruptionHandling(async () => {
      const reason = new Error("Execution stopped") as Error & {
        interruptionType: string;
        executionId: string;
      };
      reason.interruptionType = "STOP";
      reason.executionId = "exec-1";
      controller.abort(reason);

      const abortError = new Error("aborted") as Error & { name: string };
      abortError.name = "AbortError";
      throw abortError;
    }, controller.signal);

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.interruption.type).toBe("stopped");
    }
  });

  it("should work without signal (uses never-abort signal internally)", async () => {
    const result = await executeWithInterruptionHandling(async () => 42);
    expect(result).toEqual({ success: true, result: 42 });
  });
});

describe("iterateWithInterruptionHandling", () => {
  it("should yield values from async iterable", async () => {
    async function* source() {
      yield 1;
      yield 2;
      yield 3;
    }

    const results: number[] = [];
    for await (const item of iterateWithInterruptionHandling(source())) {
      if (item.type === "value") {
        results.push(item.value);
      }
    }

    expect(results).toEqual([1, 2, 3]);
  });

  it("should detect pre-check interruption", async () => {
    const controller = new AbortController();

    async function* source() {
      yield 1;
      yield 2;
    }

    const reason = new Error("Execution paused") as Error & {
      interruptionType: string;
    };
    reason.interruptionType = "PAUSE";
    controller.abort(reason);

    const results: any[] = [];
    for await (const item of iterateWithInterruptionHandling(source(), controller.signal)) {
      results.push(item);
    }

    // Since check happens before first yield and signal is already aborted
    expect(results).toHaveLength(1);
    expect(results[0].type).toBe("interrupted");
  });

  it("should check interruption at configured frequency", async () => {
    const controller = new AbortController();

    async function* source() {
      yield "a";
      yield "b";
      yield "c";
      yield "d";
    }

    const results: string[] = [];
    for await (const item of iterateWithInterruptionHandling(source(), controller.signal, { checkFrequency: 2 })) {
      if (item.type === "value") {
        results.push(item.value);
      }
    }

    // Should yield all values since signal is not aborted
    expect(results).toEqual(["a", "b", "c", "d"]);
  });

  it("should handle AbortError from iterator.next()", async () => {
    const controller = new AbortController();

    let callCount = 0;
    const asyncIterable = {
      [Symbol.asyncIterator]: () => ({
        next: async () => {
          callCount++;
          if (callCount === 2) {
            const reason = new Error("Execution stopped") as Error & {
              interruptionType: string;
            };
            reason.interruptionType = "STOP";
            controller.abort(reason);

            const err = new Error("abort") as Error & { name: string };
            err.name = "AbortError";
            throw err;
          }
          return { value: "data", done: false };
        },
      }),
    };

    const results: any[] = [];
    for await (const item of iterateWithInterruptionHandling(asyncIterable, controller.signal)) {
      results.push(item);
    }

    expect(results).toHaveLength(2);
    expect(results[0].type).toBe("value");
    expect(results[1].type).toBe("interrupted");
  });

  it("should re-throw non-abort errors from iterator", async () => {
    const controller = new AbortController();

    const asyncIterable = {
      [Symbol.asyncIterator]: () => ({
        next: async () => {
          throw new Error("unexpected");
        },
      }),
    };

    const iterator = iterateWithInterruptionHandling(asyncIterable, controller.signal);
    await expect(iterator.next()).rejects.toThrow("unexpected");
  });

  it("should work with empty iterable", async () => {
    async function* empty() {}

    const results: any[] = [];
    for await (const item of iterateWithInterruptionHandling(empty())) {
      results.push(item);
    }

    expect(results).toEqual([]);
  });

  it("should work without signal", async () => {
    async function* source() {
      yield "no-signal";
    }

    const results: any[] = [];
    for await (const item of iterateWithInterruptionHandling(source())) {
      results.push(item);
    }

    expect(results).toHaveLength(1);
    expect(results[0].type).toBe("value");
    expect(results[0].value).toBe("no-signal");
  });
});