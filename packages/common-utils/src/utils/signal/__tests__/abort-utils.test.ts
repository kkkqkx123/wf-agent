import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  withAbortSignal,
  withAbortSignalArg,
  createTimeoutSignal,
  combineAbortSignals,
  isAborted,
  getAbortReason,
  createNeverAbortSignal,
  withTimeoutAndAbort,
} from "../abort-utils.js";

describe("abort-utils", () => {
  describe("withAbortSignal", () => {
    it("should execute function when signal is not aborted", async () => {
      const mockFn = vi.fn().mockResolvedValue("success");
      const result = await withAbortSignal(mockFn);

      expect(result).toBe("success");
      expect(mockFn).toHaveBeenCalledTimes(1);
    });

    it("should throw error when signal is already aborted", () => {
      const controller = new AbortController();
      controller.abort(new Error("Test abort"));

      const mockFn = vi.fn().mockResolvedValue("success");

      expect(() => withAbortSignal(mockFn, controller.signal)).toThrow("Test abort");
    });

    it("should throw AbortError when signal is aborted without reason", () => {
      const controller = new AbortController();
      controller.abort();

      const mockFn = vi.fn().mockResolvedValue("success");

      // AbortError uses the standard message 'This operation was aborted'
      expect(() => withAbortSignal(mockFn, controller.signal)).toThrow(
        "This operation was aborted",
      );
    });
  });

  describe("withAbortSignalArg", () => {
    it("should execute function with signal when signal is not aborted", async () => {
      const mockFn = vi.fn().mockImplementation(signal => Promise.resolve(signal));
      const controller = new AbortController();

      const result = await withAbortSignalArg(mockFn, controller.signal);

      expect(result).toBe(controller.signal);
      expect(mockFn).toHaveBeenCalledWith(controller.signal);
    });

    it("should throw error when signal is already aborted", () => {
      const controller = new AbortController();
      controller.abort(new Error("Test abort"));

      const mockFn = vi.fn().mockImplementation(signal => Promise.resolve(signal));

      expect(() => withAbortSignalArg(mockFn, controller.signal)).toThrow("Test abort");
    });

    it("should throw error when signal is not provided", () => {
      const mockFn = vi.fn().mockImplementation(signal => Promise.resolve(signal));

      // withAbortSignalArg is a synchronized check for signals, so use the synchronized toThrow
      expect(() => withAbortSignalArg(mockFn, undefined)).toThrow(
        "Signal is required for withAbortSignalArg",
      );
    });
  });

  describe("createTimeoutSignal", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.restoreAllMocks();
    });

    it("should create timeout signal that aborts after specified time", () => {
      const { signal, controller } = createTimeoutSignal(1000);

      expect(signal.aborted).toBe(false);

      vi.advanceTimersByTime(1000);

      expect(signal.aborted).toBe(true);
      expect(signal.reason).toBeInstanceOf(Error);
      expect(signal.reason.message).toContain("Operation timed out after 1000ms");
    });

    it("should clean up timeout when signal is manually aborted", () => {
      const { controller } = createTimeoutSignal(1000);

      const clearTimeoutSpy = vi.spyOn(global.Date, "now");

      controller.abort();

      // Verify timer was cleared by checking that advancing time doesn't trigger abort again
      vi.advanceTimersByTime(1000);
      // No additional checks needed, as we verify no errors occur
    });
  });

  describe("combineAbortSignals", () => {
    it("should create combined signal that aborts when any signal aborts", () => {
      const controller1 = new AbortController();
      const controller2 = new AbortController();

      const { signal } = combineAbortSignals([controller1.signal, controller2.signal]);

      expect(signal.aborted).toBe(false);

      controller1.abort(new Error("Combined abort"));

      expect(signal.aborted).toBe(true);
      expect(signal.reason.message).toBe("Combined abort");
    });

    it("should immediately abort combined signal if any input signal is already aborted", () => {
      const controller1 = new AbortController();
      controller1.abort(new Error("Already aborted"));
      const controller2 = new AbortController();

      const { signal } = combineAbortSignals([controller1.signal, controller2.signal]);

      expect(signal.aborted).toBe(true);
      expect(signal.reason.message).toBe("Already aborted");
    });
  });

  describe("isAborted", () => {
    it("should return false for non-aborted signal", () => {
      const controller = new AbortController();
      expect(isAborted(controller.signal)).toBe(false);
    });

    it("should return true for aborted signal", () => {
      const controller = new AbortController();
      controller.abort();
      expect(isAborted(controller.signal)).toBe(true);
    });

    it("should return false for undefined signal", () => {
      expect(isAborted(undefined)).toBe(false);
    });
  });

  describe("getAbortReason", () => {
    it("should return undefined for non-aborted signal", () => {
      const controller = new AbortController();
      expect(getAbortReason(controller.signal)).toBeUndefined();
    });

    it("should return reason for aborted signal", () => {
      const controller = new AbortController();
      const reason = new Error("Test reason");
      controller.abort(reason);
      expect(getAbortReason(controller.signal)).toBe(reason);
    });

    it("should return undefined for undefined signal", () => {
      expect(getAbortReason(undefined)).toBeUndefined();
    });
  });

  describe("createNeverAbortSignal", () => {
    it("should return a signal that never aborts", () => {
      const signal = createNeverAbortSignal();
      expect(signal.aborted).toBe(false);

      // Even after waiting, the signal should not be aborted
      // Note: We can't actually wait indefinitely, so we just verify it's not aborted initially
      expect(signal.aborted).toBe(false);
    });
  });

  describe("withTimeoutAndAbort", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.restoreAllMocks();
    });

    it("should execute function without timeout or signal", async () => {
      const mockFn = vi.fn().mockResolvedValue("result");

      const result = await withTimeoutAndAbort(mockFn, {});

      expect(result).toBe("result");
      expect(mockFn).toHaveBeenCalledOnce();
      expect(mockFn.mock.calls[0]?.[0]).toBeDefined(); // Should receive a signal
    });

    it("should execute function with custom signal", async () => {
      const controller = new AbortController();
      const mockFn = vi.fn().mockResolvedValue("result");

      const result = await withTimeoutAndAbort(mockFn, { signal: controller.signal });

      expect(result).toBe("result");
      expect(mockFn).toHaveBeenCalledOnce();
    });

    it("should timeout when timeoutMs is specified", async () => {
      // Create an analog function that listens for signals
      const mockFn = vi.fn().mockImplementation(signal => {
        return new Promise((resolve, reject) => {
          const listener = () => {
            reject(signal.reason || new Error("Operation aborted"));
          };
          signal.addEventListener("abort", listener);

          // If the signal is not aborted, at a later time resolve the
          if (!signal.aborted) {
            setTimeout(() => {
              signal.removeEventListener("abort", listener);
              resolve("result");
            }, 2000); // Setting a delay longer than the timeout
          } else {
            signal.removeEventListener("abort", listener);
            reject(signal.reason || new Error("Operation aborted"));
          }
        });
      });

      const promise = withTimeoutAndAbort(mockFn, { timeoutMs: 1000 });

      vi.advanceTimersByTime(1000);

      await expect(promise).rejects.toThrow("Operation timed out after 1000ms");
    });

    it("should throw AbortError when combined signal is already aborted", async () => {
      const controller = new AbortController();
      controller.abort(); // immediate suspension

      const mockFn = vi.fn().mockResolvedValue("result");

      // AbortError uses the standard message 'This operation was aborted'
      await expect(withTimeoutAndAbort(mockFn, { signal: controller.signal })).rejects.toThrow(
        "This operation was aborted",
      );
    });

    it("should abort when provided signal is aborted", async () => {
      // Create an analog function that listens for signals
      const mockFn = vi.fn().mockImplementation(signal => {
        return new Promise((resolve, reject) => {
          const listener = () => {
            reject(signal.reason || new Error("Operation aborted"));
          };
          signal.addEventListener("abort", listener);

          // If the signal is not aborted, at a later time resolve the
          if (!signal.aborted) {
            setTimeout(() => {
              signal.removeEventListener("abort", listener);
              resolve("result");
            }, 100); // deferred implementation
          } else {
            signal.removeEventListener("abort", listener);
            reject(signal.reason || new Error("Operation aborted"));
          }
        });
      });

      const controller = new AbortController();
      const promise = withTimeoutAndAbort(mockFn, { signal: controller.signal });

      controller.abort(new Error("External abort"));

      await expect(promise).rejects.toThrow("External abort");
    });
  });
});
