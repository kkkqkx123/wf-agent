/**
 * Unit Tests for AbortSignal Utilities
 */

import { describe, it, expect } from "vitest";
import {
  isAborted,
  createNeverAbortSignal,
  combineAbortSignals,
  withAbortSignal,
  checkInterruption,
  type WithAbortSignalResult,
} from "../abort-signal-utils.js";

describe("isAborted", () => {
  it("should return false for undefined signal", () => {
    expect(isAborted(undefined)).toBe(false);
  });

  it("should return false for non-aborted signal", () => {
    const controller = new AbortController();
    expect(isAborted(controller.signal)).toBe(false);
  });

  it("should return true for aborted signal", () => {
    const controller = new AbortController();
    controller.abort();
    expect(isAborted(controller.signal)).toBe(true);
  });
});

describe("createNeverAbortSignal", () => {
  it("should return a signal that never aborts", () => {
    const signal = createNeverAbortSignal();
    expect(signal.aborted).toBe(false);
  });

  it("should return the same signal instance (singleton pattern)", () => {
    const signal1 = createNeverAbortSignal();
    const signal2 = createNeverAbortSignal();
    expect(signal1).toBe(signal2);
  });
});

describe("combineAbortSignals", () => {
  it("should return never-abort signal for empty array", () => {
    const signal = combineAbortSignals([]);
    expect(signal.aborted).toBe(false);
  });

  it("should return never-abort signal for all-undefined array", () => {
    const signal = combineAbortSignals([undefined, undefined]);
    expect(signal.aborted).toBe(false);
  });

  it("should return the same signal when only one valid signal", () => {
    const controller = new AbortController();
    const signal = combineAbortSignals([controller.signal]);
    expect(signal).toBe(controller.signal);
  });

  it("should abort when any signal aborts", () => {
    const c1 = new AbortController();
    const c2 = new AbortController();
    const combined = combineAbortSignals([c1.signal, c2.signal]);

    c1.abort();
    expect(combined.aborted).toBe(true);
  });

  it("should abort immediately if any signal already aborted", () => {
    const c1 = new AbortController();
    const c2 = new AbortController();
    c2.abort("test reason");

    const combined = combineAbortSignals([c1.signal, c2.signal]);
    expect(combined.aborted).toBe(true);
    expect(combined.reason).toBe("test reason");
  });

  it("should propagate the abort reason", () => {
    const c1 = new AbortController();
    const c2 = new AbortController();
    const combined = combineAbortSignals([c1.signal, c2.signal]);

    const reason = new Error("custom reason");
    c1.abort(reason);
    expect(combined.reason).toBe(reason);
  });

  it("should use AbortSignal.any when available", () => {
    // Verify that AbortSignal.any is available in Node.js 22+
    expect(typeof AbortSignal.any).toBe("function");

    const c1 = new AbortController();
    const c2 = new AbortController();
    const combined = combineAbortSignals([c1.signal, c2.signal]);

    c1.abort("from any");
    expect(combined.aborted).toBe(true);
  });
});

describe("checkInterruption", () => {
  it("should return continue for undefined signal", () => {
    expect(checkInterruption(undefined)).toEqual({ type: "continue" });
  });

  it("should return continue for non-aborted signal", () => {
    const controller = new AbortController();
    expect(checkInterruption(controller.signal)).toEqual({ type: "continue" });
  });

  it("should return aborted with reason for aborted signal", () => {
    const controller = new AbortController();
    const reason = new Error("paused");
    controller.abort(reason);
    const result = checkInterruption(controller.signal);
    expect(result).toEqual({ type: "aborted", reason });
  });

  it("should return aborted with undefined reason when no reason given", () => {
    const controller = new AbortController();
    controller.abort();
    const result = checkInterruption(controller.signal);
    expect(result.type).toBe("aborted");
  });
});

describe("withAbortSignal", () => {
  it("should return ok with value when operation succeeds", async () => {
    const result = await withAbortSignal(async (signal) => "success");
    expect(result).toEqual({ ok: true, value: "success" });
  });

  it("should return ok with value when signal is provided and not aborted", async () => {
    const controller = new AbortController();
    const result = await withAbortSignal(async (signal) => {
      expect(signal).toBe(controller.signal);
      return 42;
    }, controller.signal);
    expect(result).toEqual({ ok: true, value: 42 });
  });

  it("should pass signal parameter to function for periodic checks", async () => {
    const controller = new AbortController();
    let receivedSignal: AbortSignal | undefined;

    await withAbortSignal(async (signal) => {
      receivedSignal = signal;
      return "done";
    }, controller.signal);

    expect(receivedSignal).toBe(controller.signal);
  });

  it("should return error when signal is already aborted before execution", async () => {
    const controller = new AbortController();
    const reason = new Error("Already stopped");
    controller.abort(reason);

    const result = await withAbortSignal(async (signal) => "won't run", controller.signal);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("Already stopped");
      expect(result.isAborted).toBe(true);
    }
  });

  it("should return error if signal aborts after execution completes", async () => {
    const controller = new AbortController();

    const result = await withAbortSignal(async (signal) => {
      controller.abort(new Error("paused after result"));
      return "done";
    }, controller.signal);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("paused after result");
      expect(result.isAborted).toBe(true);
    }
  });

  it("should return wrapped error when operation throws non-abort error", async () => {
    const controller = new AbortController();
    const result = await withAbortSignal(async (signal) => {
      throw new Error("operation failed");
    }, controller.signal);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("operation failed");
      expect(result.isAborted).toBe(false);
    }
  });

  it("should preserve cause chain in error", async () => {
    const controller = new AbortController();
    const originalError = new Error("original reason");
    controller.abort(originalError);

    const result = await withAbortSignal(async (signal) => "fail", controller.signal);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.cause).toBe(originalError);
      expect(result.isAborted).toBe(true);
    }
  });

  it("should support periodic interruption checks within function", async () => {
    const controller = new AbortController();
    let iterationsRun = 0;

    const result = await withAbortSignal(
      async (signal) => {
        for (let i = 0; i < 1000; i++) {
          if (signal?.aborted) {
            break;
          }
          iterationsRun++;
          if (i === 10) {
            controller.abort(new Error("interrupted at iteration 10"));
          }
        }
        return iterationsRun;
      },
      controller.signal,
    );

    expect(iterationsRun).toBeLessThan(100);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.isAborted).toBe(true);
    }
  });

  it("should distinguish between interruption and business errors", async () => {
    const controller = new AbortController();

    // Test business error case
    const businessErrorResult = await withAbortSignal(async (signal) => {
      throw new Error("business logic failed");
    }, controller.signal);

    expect(businessErrorResult.ok).toBe(false);
    if (!businessErrorResult.ok) {
      expect(businessErrorResult.error.message).toBe("business logic failed");
      expect(businessErrorResult.isAborted).toBe(false);
    }

    // Test interruption case
    controller.abort(new Error("user cancelled"));
    const abortedResult = await withAbortSignal(async (signal) => "should not run", controller.signal);

    expect(abortedResult.ok).toBe(false);
    if (!abortedResult.ok) {
      expect(abortedResult.error.message).toBe("user cancelled");
      expect(abortedResult.isAborted).toBe(true);
    }
  });

  it("should handle async operations with interruption", async () => {
    const controller = new AbortController();
    let completedOperations = 0;

    const result = await withAbortSignal(
      async (signal) => {
        for (let i = 0; i < 5; i++) {
          if (signal?.aborted) break;

          await new Promise((resolve) => setTimeout(resolve, 10));
          completedOperations++;

          if (i === 2) {
            controller.abort(new Error("interrupted after 3rd operation"));
          }
        }
        return completedOperations;
      },
      controller.signal,
    );

    expect(completedOperations).toBeLessThanOrEqual(3);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.isAborted).toBe(true);
    }
  });

  it("should handle timeout combined with other signals", async () => {
    const abortController = new AbortController();
    const timeoutSignal = AbortSignal.timeout(500);
    const combined = combineAbortSignals([abortController.signal, timeoutSignal]);

    // Should complete before timeout
    const result = await withAbortSignal(
      async (signal) => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return "completed";
      },
      combined,
    );

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value).toBe("completed");
    }
  });
});

