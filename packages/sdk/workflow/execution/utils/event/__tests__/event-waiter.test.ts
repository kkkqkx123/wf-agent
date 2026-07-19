/**
 * Event Waiter Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  waitForWorkflowExecutionPaused,
  waitForWorkflowExecutionCancelled,
  waitForWorkflowExecutionCompleted,
  waitForWorkflowExecutionFailed,
  waitForWorkflowExecutionResumed,
  waitForAnyLifecycleEvent,
  waitForMultipleWorkflowExecutionsCompleted,
  waitForAnyWorkflowExecutionCompleted,
  waitForAnyWorkflowExecutionCompletion,
  waitForNodeCompleted,
  waitForNodeFailed,
  WAIT_FOREVER,
} from "../../event-waiter.js";
import type { EventRegistry } from "../../../../../shared/registry/event-registry.js";

// Mock the timeout utilities
vi.mock("../../../../shared/utils/timeout/timeout-utils.js", () => ({
  executeWithSharedTimeout: vi.fn(async (config, _timeout, _options) => {
    // Simulate shared timeout execution
    await config.wait();
  }),
}));

// Mock the timeout config
vi.mock("../../../../api/shared/config/index.js", () => ({
  mergeTimeoutWithDefaults: vi.fn(() => ({
    workflowExecutionPause: 5000,
    workflowExecutionCompletion: 30000,
    workflowExecutionResume: 5000,
    lifecycleEvent: 5000,
    nodeCompletion: 30000,
    nodeFailed: 30000,
  })),
}));

describe("Event Waiter Functions", () => {
  let mockEventManager: EventRegistry;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEventManager = {
      waitFor: vi.fn(),
      emit: vi.fn(),
    } as unknown as EventRegistry;
  });

  describe("waitForWorkflowExecutionPaused", () => {
    it("should wait for WORKFLOW_EXECUTION_PAUSED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionPaused(mockEventManager, "exec-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_PAUSED",
        "exec-1",
        5000,
        expect.any(Function),
      );
    });

    it("should use custom timeout", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionPaused(mockEventManager, "exec-1", 10000);

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_PAUSED",
        "exec-1",
        10000,
        expect.any(Function),
      );
    });

    it("should wait indefinitely with WAIT_FOREVER", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionPaused(mockEventManager, "exec-1", WAIT_FOREVER);

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_PAUSED",
        "exec-1",
        undefined,
        expect.any(Function),
      );
    });

    it("should filter by executionId", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionPaused(mockEventManager, "exec-1");

      const filterFn = (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mock.calls[0]![3];
      expect(filterFn({ executionId: "exec-1" })).toBe(true);
      expect(filterFn({ executionId: "exec-2" })).toBe(false);
    });
  });

  describe("waitForWorkflowExecutionCancelled", () => {
    it("should wait for WORKFLOW_EXECUTION_CANCELLED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionCancelled(mockEventManager, "exec-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_CANCELLED",
        "exec-1",
        5000,
        expect.any(Function),
      );
    });
  });

  describe("waitForWorkflowExecutionCompleted", () => {
    it("should wait for WORKFLOW_EXECUTION_COMPLETED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionCompleted(mockEventManager, "exec-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_COMPLETED",
        "exec-1",
        30000,
        expect.any(Function),
      );
    });

    it("should use custom timeout", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionCompleted(mockEventManager, "exec-1", 60000);

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_COMPLETED",
        "exec-1",
        60000,
        expect.any(Function),
      );
    });
  });

  describe("waitForWorkflowExecutionFailed", () => {
    it("should wait for WORKFLOW_EXECUTION_FAILED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionFailed(mockEventManager, "exec-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_FAILED",
        "exec-1",
        30000,
        expect.any(Function),
      );
    });
  });

  describe("waitForWorkflowExecutionResumed", () => {
    it("should wait for WORKFLOW_EXECUTION_RESUMED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForWorkflowExecutionResumed(mockEventManager, "exec-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "WORKFLOW_EXECUTION_RESUMED",
        "exec-1",
        5000,
        expect.any(Function),
      );
    });
  });

  describe("waitForAnyLifecycleEvent", () => {
    it("should wait for any lifecycle event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForAnyLifecycleEvent(mockEventManager, "exec-1");

      // Should call waitFor for each lifecycle event type
      expect(mockEventManager.waitFor).toHaveBeenCalledTimes(5);
    });

    it("should use Promise.race for any event", async () => {
      // First event resolves immediately
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>)
        .mockResolvedValueOnce(undefined)
        .mockImplementation(() => new Promise(() => {})); // Others never resolve

      await waitForAnyLifecycleEvent(mockEventManager, "exec-1");

      // Should have called waitFor for multiple events
      expect(mockEventManager.waitFor).toHaveBeenCalled();
    });
  });

  describe("waitForMultipleWorkflowExecutionsCompleted", () => {
    it("should wait for all executions to complete (individual timeout)", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForMultipleWorkflowExecutionsCompleted(
        mockEventManager,
        ["exec-1", "exec-2", "exec-3"],
        30000,
        { timeoutMode: "individual" },
      );

      expect(mockEventManager.waitFor).toHaveBeenCalledTimes(3);
    });

    it("should wait for all executions to complete (shared timeout)", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForMultipleWorkflowExecutionsCompleted(
        mockEventManager,
        ["exec-1", "exec-2"],
        30000,
        { timeoutMode: "shared" },
      );

      // With shared timeout, should use executeWithSharedTimeout
      expect(mockEventManager.waitFor).toHaveBeenCalled();
    });

    it("should use default timeout mode when not specified", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForMultipleWorkflowExecutionsCompleted(mockEventManager, ["exec-1", "exec-2"]);

      expect(mockEventManager.waitFor).toHaveBeenCalled();
    });
  });

  describe("waitForAnyWorkflowExecutionCompleted", () => {
    it("should wait for any execution to complete", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const result = await waitForAnyWorkflowExecutionCompleted(mockEventManager, [
        "exec-1",
        "exec-2",
        "exec-3",
      ]);

      expect(result).toBe("exec-1"); // First one resolves
    });

    it("should return the completed execution ID", async () => {
      // Second execution completes first
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>)
        .mockImplementationOnce(() => new Promise(() => {}))
        .mockResolvedValueOnce(undefined)
        .mockImplementationOnce(() => new Promise(() => {}));

      const result = await waitForAnyWorkflowExecutionCompleted(mockEventManager, [
        "exec-1",
        "exec-2",
        "exec-3",
      ]);

      expect(result).toBe("exec-2");
    });
  });

  describe("waitForAnyWorkflowExecutionCompletion", () => {
    it("should wait for any execution to complete or fail", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const result = await waitForAnyWorkflowExecutionCompletion(mockEventManager, [
        "exec-1",
        "exec-2",
      ]);

      expect(result).toEqual({
        executionId: "exec-1",
        status: "COMPLETED",
      });
    });

    it("should return FAILED status when execution fails", async () => {
      // First execution fails
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>)
        .mockImplementationOnce(() => new Promise(() => {})) // completed never resolves
        .mockResolvedValueOnce(undefined) // failed resolves first
        .mockImplementationOnce(() => new Promise(() => {}))
        .mockImplementationOnce(() => new Promise(() => {}));

      const result = await waitForAnyWorkflowExecutionCompletion(mockEventManager, ["exec-1"]);

      expect(result.status).toBe("FAILED");
    });
  });

  describe("waitForNodeCompleted", () => {
    it("should wait for NODE_COMPLETED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForNodeCompleted(mockEventManager, "exec-1", "node-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "NODE_COMPLETED",
        "exec-1",
        30000,
        expect.any(Function),
      );
    });

    it("should filter by executionId and nodeId", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForNodeCompleted(mockEventManager, "exec-1", "node-1");

      const filterFn = (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mock.calls[0]![3];
      expect(filterFn({ executionId: "exec-1", nodeId: "node-1" })).toBe(true);
      expect(filterFn({ executionId: "exec-1", nodeId: "node-2" })).toBe(false);
      expect(filterFn({ executionId: "exec-2", nodeId: "node-1" })).toBe(false);
    });

    it("should use custom timeout", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForNodeCompleted(mockEventManager, "exec-1", "node-1", 60000);

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "NODE_COMPLETED",
        "exec-1",
        60000,
        expect.any(Function),
      );
    });
  });

  describe("waitForNodeFailed", () => {
    it("should wait for NODE_FAILED event", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForNodeFailed(mockEventManager, "exec-1", "node-1");

      expect(mockEventManager.waitFor).toHaveBeenCalledWith(
        "NODE_FAILED",
        "exec-1",
        30000,
        expect.any(Function),
      );
    });

    it("should filter by executionId and nodeId", async () => {
      (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      await waitForNodeFailed(mockEventManager, "exec-1", "node-1");

      const filterFn = (mockEventManager.waitFor as ReturnType<typeof vi.fn>).mock.calls[0]![3];
      expect(filterFn({ executionId: "exec-1", nodeId: "node-1" })).toBe(true);
      expect(filterFn({ executionId: "exec-2", nodeId: "node-1" })).toBe(false);
    });
  });

  describe("WAIT_FOREVER constant", () => {
    it("should be -1", () => {
      expect(WAIT_FOREVER).toBe(-1);
    });
  });
});
