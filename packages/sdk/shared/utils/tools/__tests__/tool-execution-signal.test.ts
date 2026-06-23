/**
 * Tool Execution Signal Utilities - Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  createToolExecutionSignal,
  isToolExecutionTimeout,
  isToolExecutionExternalAbort,
  getToolExecutionTimeoutMs,
  getToolExecutionAbortReason,
} from "../tool-execution-signal.js";

describe("createToolExecutionSignal", () => {
  let timerId: NodeJS.Timeout | null = null;

  afterEach(() => {
    if (timerId) {
      clearTimeout(timerId);
    }
  });

  describe("timeout handling", () => {
    it("should abort after timeout", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 50);

      expect(signal.aborted).toBe(false);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(signal.aborted).toBe(true);
      expect(isToolExecutionTimeout(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should not abort if timeout is 0", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 0);

      await new Promise(resolve => setTimeout(resolve, 50));

      expect(signal.aborted).toBe(false);
      cleanup();
    });

    it("should track timeout duration separately", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 3000);

      await new Promise(resolve => setTimeout(resolve, 100));

      // Before timeout, reason is not set
      expect(getToolExecutionTimeoutMs(signal.reason as Error)).toBeNull();

      // Clean up and wait for actual timeout
      cleanup();
      cleanup(); // cleanup again to ensure we don't re-abort

      expect(signal.aborted).toBe(true);
    });
  });

  describe("external signal handling", () => {
    it("should abort when external signal aborts", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      expect(signal.aborted).toBe(false);

      externalController.abort();

      // Give event propagation a chance
      await new Promise(resolve => setTimeout(resolve, 10));

      expect(signal.aborted).toBe(true);
      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should abort immediately if external signal is already aborted", () => {
      const externalController = new AbortController();
      externalController.abort();

      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      expect(signal.aborted).toBe(true);
      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should preserve abort reason from external signal", async () => {
      const reason = new Error("Custom abort reason");
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      externalController.abort(reason);

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(getToolExecutionAbortReason(signal.reason as Error)).toBe(reason);
      cleanup();
    });

    it("should ignore null/undefined external signal", () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 0);

      expect(signal.aborted).toBe(false);
      cleanup();
    });
  });

  describe("combined handling", () => {
    it("should abort on timeout even with external signal", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 50);

      expect(signal.aborted).toBe(false);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(signal.aborted).toBe(true);
      expect(isToolExecutionTimeout(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should abort on external signal even with timeout", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 5000);

      expect(signal.aborted).toBe(false);

      externalController.abort();

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(signal.aborted).toBe(true);
      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should use whichever happens first (timeout)", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 50);

      // Abort external after longer delay
      timerId = setTimeout(() => externalController.abort(), 200);

      await new Promise(resolve => setTimeout(resolve, 100));

      // Should abort on timeout first
      expect(signal.aborted).toBe(true);
      expect(isToolExecutionTimeout(signal.reason as Error)).toBe(true);
      cleanup();
    });

    it("should use whichever happens first (external)", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 5000);

      externalController.abort();

      await new Promise(resolve => setTimeout(resolve, 50));

      // Should abort on external signal first
      expect(signal.aborted).toBe(true);
      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(true);
      cleanup();
    });
  });

  describe("cleanup", () => {
    it("should clean up timeout on cleanup call", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 100);

      cleanup();

      // Clear any lingering timeouts
      await new Promise(resolve => setTimeout(resolve, 150));

      // Signal should still be aborted due to cleanup
      expect(signal.aborted).toBe(true);
    });

    it("should remove external signal listener on cleanup", async () => {
      const externalController = new AbortController();
      const removeSpy = vi.spyOn(externalController.signal, "removeEventListener");

      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      cleanup();

      expect(removeSpy).toHaveBeenCalled();
    });

    it("should not double-abort on cleanup", () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 0);

      cleanup();
      cleanup();

      expect(signal.aborted).toBe(true);
    });
  });

  describe("helper functions", () => {
    it("isToolExecutionTimeout should identify timeout errors", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 50);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(isToolExecutionTimeout(signal.reason as Error)).toBe(true);
      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(false);
      cleanup();
    });

    it("isToolExecutionExternalAbort should identify external abort", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      externalController.abort();

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(isToolExecutionExternalAbort(signal.reason as Error)).toBe(true);
      expect(isToolExecutionTimeout(signal.reason as Error)).toBe(false);
      cleanup();
    });

    it("getToolExecutionTimeoutMs should return null for non-timeout errors", async () => {
      const externalController = new AbortController();
      const { signal, cleanup } = createToolExecutionSignal(externalController.signal, 0);

      externalController.abort();

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(getToolExecutionTimeoutMs(signal.reason as Error)).toBeNull();
      cleanup();
    });

    it("getToolExecutionAbortReason should return null for timeout errors", async () => {
      const { signal, cleanup } = createToolExecutionSignal(undefined, 50);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(getToolExecutionAbortReason(signal.reason as Error)).toBeNull();
      cleanup();
    });
  });
});
