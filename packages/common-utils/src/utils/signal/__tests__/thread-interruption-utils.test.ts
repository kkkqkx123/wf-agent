import { describe, it, expect, vi } from "vitest";
import {
  checkInterruption,
  shouldContinue,
  isInterrupted,
  getInterruptionType,
  getThreadId,
  getNodeId,
  createInterruptionInfo,
  getInterruptionDescription,
  withInterruptionCheck,
} from "../thread-interruption-utils.js";
import { ThreadInterruptedException } from "@wf-agent/types";

describe("thread-interruption-utils", () => {
  describe("checkInterruption", () => {
    it("should return continue when signal is not aborted", () => {
      const controller = new AbortController();

      const result = checkInterruption(controller.signal);
      expect(result).toEqual({ type: "continue" });
    });

    it("should return paused when signal has PAUSE interruption", () => {
      const controller = new AbortController();
      const threadInterrupt = new ThreadInterruptedException(
        "Test interruption",
        "PAUSE",
        "thread-1",
        "node-1",
      );
      controller.abort(threadInterrupt);

      const result = checkInterruption(controller.signal);
      expect(result).toEqual({
        type: "paused",
        threadId: "thread-1",
        nodeId: "node-1",
      });
    });

    it("should return stopped when signal has STOP interruption", () => {
      const controller = new AbortController();
      const threadInterrupt = new ThreadInterruptedException(
        "Test interruption",
        "STOP",
        "thread-1",
        "node-1",
      );
      controller.abort(threadInterrupt);

      const result = checkInterruption(controller.signal);
      expect(result).toEqual({
        type: "stopped",
        threadId: "thread-1",
        nodeId: "node-1",
      });
    });

    it("should return aborted when signal is aborted with other reason", () => {
      const controller = new AbortController();
      controller.abort(new Error("Regular error"));

      const result = checkInterruption(controller.signal);
      expect(result.type).toBe("aborted");
      if (result.type === "aborted") {
        expect(result.reason).toBeInstanceOf(Error);
      }
    });

    it("should return continue when signal is undefined", () => {
      const result = checkInterruption(undefined);
      expect(result).toEqual({ type: "continue" });
    });
  });

  describe("shouldContinue", () => {
    it("should return true for continue result", () => {
      const result = { type: "continue" as const };
      expect(shouldContinue(result)).toBe(true);
    });

    it("should return false for paused result", () => {
      const result = { type: "paused" as const, nodeId: "node-1" };
      expect(shouldContinue(result)).toBe(false);
    });

    it("should return false for stopped result", () => {
      const result = { type: "stopped" as const, nodeId: "node-1" };
      expect(shouldContinue(result)).toBe(false);
    });

    it("should return false for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test" };
      expect(shouldContinue(result)).toBe(false);
    });
  });

  describe("isInterrupted", () => {
    it("should return false for continue result", () => {
      const result = { type: "continue" as const };
      expect(isInterrupted(result)).toBe(false);
    });

    it("should return true for paused result", () => {
      const result = { type: "paused" as const, nodeId: "node-1" };
      expect(isInterrupted(result)).toBe(true);
    });

    it("should return true for stopped result", () => {
      const result = { type: "stopped" as const, nodeId: "node-1" };
      expect(isInterrupted(result)).toBe(true);
    });

    it("should return true for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test" };
      expect(isInterrupted(result)).toBe(true);
    });
  });

  describe("getInterruptionType", () => {
    it("should return PAUSE for paused result", () => {
      const result = { type: "paused" as const, nodeId: "node-1" };
      expect(getInterruptionType(result)).toBe("PAUSE");
    });

    it("should return STOP for stopped result", () => {
      const result = { type: "stopped" as const, nodeId: "node-1" };
      expect(getInterruptionType(result)).toBe("STOP");
    });

    it("should return null for continue result", () => {
      const result = { type: "continue" as const };
      expect(getInterruptionType(result)).toBeNull();
    });

    it("should return null for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test" };
      expect(getInterruptionType(result)).toBeNull();
    });
  });

  describe("getThreadId", () => {
    it("should return thread ID for paused result", () => {
      const result = { type: "paused" as const, threadId: "thread-1", nodeId: "node-1" };
      expect(getThreadId(result)).toBe("thread-1");
    });

    it("should return thread ID for stopped result", () => {
      const result = { type: "stopped" as const, threadId: "thread-1", nodeId: "node-1" };
      expect(getThreadId(result)).toBe("thread-1");
    });

    it("should return undefined for continue result", () => {
      const result = { type: "continue" as const };
      expect(getThreadId(result)).toBeUndefined();
    });

    it("should return undefined for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test" };
      expect(getThreadId(result)).toBeUndefined();
    });
  });

  describe("getNodeId", () => {
    it("should return node ID for paused result", () => {
      const result = { type: "paused" as const, threadId: "thread-1", nodeId: "node-1" };
      expect(getNodeId(result)).toBe("node-1");
    });

    it("should return node ID for stopped result", () => {
      const result = { type: "stopped" as const, threadId: "thread-1", nodeId: "node-1" };
      expect(getNodeId(result)).toBe("node-1");
    });

    it("should return undefined for continue result", () => {
      const result = { type: "continue" as const };
      expect(getNodeId(result)).toBeUndefined();
    });

    it("should return undefined for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test" };
      expect(getNodeId(result)).toBeUndefined();
    });
  });

  describe("createInterruptionInfo", () => {
    it("should create interruption info with PAUSE type", () => {
      const info = createInterruptionInfo("PAUSE", "thread-1", "node-1");

      expect(info.type).toBe("PAUSE");
      expect(info.threadId).toBe("thread-1");
      expect(info.nodeId).toBe("node-1");
      expect(info.timestamp).toBeDefined();
    });

    it("should create interruption info with STOP type", () => {
      const info = createInterruptionInfo("STOP", "thread-2", "node-2");

      expect(info.type).toBe("STOP");
      expect(info.threadId).toBe("thread-2");
      expect(info.nodeId).toBe("node-2");
    });
  });

  describe("getInterruptionDescription", () => {
    it("should return description for paused result", () => {
      const result = { type: "paused" as const, nodeId: "node-1" };
      const description = getInterruptionDescription(result);

      expect(description).toBe("Thread paused at node: node-1");
    });

    it("should return description for stopped result", () => {
      const result = { type: "stopped" as const, nodeId: "node-1" };
      const description = getInterruptionDescription(result);

      expect(description).toBe("Thread stopped at node: node-1");
    });

    it("should return description for continue result", () => {
      const result = { type: "continue" as const };
      const description = getInterruptionDescription(result);

      expect(description).toBe("Execution continuing");
    });

    it("should return description for aborted result", () => {
      const result = { type: "aborted" as const, reason: "test reason" };
      const description = getInterruptionDescription(result);

      expect(description).toBe("test reason");
    });
  });

  describe("withInterruptionCheck", () => {
    it("should execute function when signal is not aborted", async () => {
      const controller = new AbortController();
      const mockFn = vi.fn().mockResolvedValue("success");

      const result = await withInterruptionCheck(mockFn, controller.signal);

      expect(result).toEqual({ result: "success", status: "completed" });
      expect(mockFn).toHaveBeenCalledTimes(1);
    });

    it("should return interrupted when signal has PAUSE interruption", async () => {
      const controller = new AbortController();
      const threadInterrupt = new ThreadInterruptedException(
        "Test interruption",
        "PAUSE",
        "thread-1",
        "node-1",
      );
      controller.abort(threadInterrupt);

      const mockFn = vi.fn().mockResolvedValue("success");

      const result = await withInterruptionCheck(mockFn, controller.signal);

      expect(result.status).toBe("interrupted");
      if (result.status === "interrupted") {
        expect(result.interruption.type).toBe("paused");
      }
      expect(mockFn).not.toHaveBeenCalled();
    });

    it("should return interrupted when signal has STOP interruption", async () => {
      const controller = new AbortController();
      const threadInterrupt = new ThreadInterruptedException(
        "Test interruption",
        "STOP",
        "thread-1",
        "node-1",
      );
      controller.abort(threadInterrupt);

      const mockFn = vi.fn().mockResolvedValue("success");

      const result = await withInterruptionCheck(mockFn, controller.signal);

      expect(result.status).toBe("interrupted");
      if (result.status === "interrupted") {
        expect(result.interruption.type).toBe("stopped");
      }
      expect(mockFn).not.toHaveBeenCalled();
    });

    it("should handle AbortError from function", async () => {
      const controller = new AbortController();
      const abortError = new Error("AbortError");
      abortError.name = "AbortError";
      const mockFn = vi.fn().mockRejectedValue(abortError);

      const result = await withInterruptionCheck(mockFn, controller.signal);

      // When the function throws an AbortError, it should return an interrupted status.
      expect(result.status).toBe("interrupted");
      expect(mockFn).toHaveBeenCalledTimes(1);
    });

    it("should throw non-abort errors", async () => {
      const controller = new AbortController();
      const mockFn = vi.fn().mockRejectedValue(new Error("Regular error"));

      await expect(withInterruptionCheck(mockFn, controller.signal)).rejects.toThrow(
        "Regular error",
      );
    });
  });
});
