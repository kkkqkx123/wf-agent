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
    const result = await withAbortSignal(async () => "success");
    expect(result).toEqual({ ok: true, value: "success" });
  });

  it("should return ok with value when signal is provided and not aborted", async () => {
    const controller = new AbortController();
    const result = await withAbortSignal(async () => 42, controller.signal);
    expect(result).toEqual({ ok: true, value: 42 });
  });

  it("should return error when signal is already aborted before execution", async () => {
    const controller = new AbortController();
    const reason = new Error("Already stopped");
    controller.abort(reason);

    const result = await withAbortSignal(async () => "won't run", controller.signal);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("Already stopped");
    }
  });

  it("should return error if signal aborts after execution completes", async () => {
    // This tests the post-execution check
    const controller = new AbortController();

    const result = await withAbortSignal(async () => {
      controller.abort(new Error("paused after result"));
      return "done";
    }, controller.signal);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("paused after result");
    }
  });

  it("should return wrapped error when operation throws non-abort error", async () => {
    const controller = new AbortController();
    const result = await withAbortSignal(async () => {
      throw new Error("operation failed");
    }, controller.signal);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toBe("operation failed");
    }
  });

  it("should preserve cause chain in error", async () => {
    const controller = new AbortController();
    const originalError = new Error("original reason");
    controller.abort(originalError);

    const result = await withAbortSignal(async () => "fail", controller.signal);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.cause).toBe(originalError);
    }
  });
});
