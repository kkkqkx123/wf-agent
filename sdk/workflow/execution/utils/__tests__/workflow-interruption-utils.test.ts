/**
 * Workflow Interruption Utils Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the base interruption check - must be at the top level before any imports
const mockCheckExecutionInterruption = vi.hoisted(() => vi.fn());
const mockGetExecutionInterruptionDescription = vi.hoisted(() =>
  vi.fn((result: Record<string, unknown>) => {
    switch (result.type) {
      case "continue":
        return "Workflow execution continuing";
      case "paused":
        return `Workflow execution paused at node: ${result.nodeId}`;
      case "stopped":
        return `Workflow execution stopped at node: ${result.nodeId}`;
      case "aborted":
        return result.reason ? String(result.reason) : "Workflow execution operation aborted";
      default:
        return "Unknown workflow interruption state";
    }
  }),
);
vi.mock("../../../../core/utils/interruption/index.js", () => ({
  checkExecutionInterruption: mockCheckExecutionInterruption,
  getExecutionInterruptionDescription: mockGetExecutionInterruptionDescription,
}));

import {
  checkWorkflowInterruption,
  getWorkflowInterruptionType,
  getWorkflowInterruptionDescription,
  toWorkflowInterruptionResult,
  createWorkflowInterruptionAbortReason,
} from "../workflow-interruption-utils.js";
import { WorkflowExecutionInterruptedException } from "../../types/workflow-interruption-types.js";

describe("checkWorkflowInterruption", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("without signal", () => {
    it("should return continue when no signal provided", () => {
      mockCheckExecutionInterruption.mockReturnValue({ type: "continue" });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("continue");
    });
  });

  describe("with non-aborted signal", () => {
    it("should return continue when signal is not aborted", () => {
      mockCheckExecutionInterruption.mockReturnValue({ type: "continue" });

      const signal = new AbortController().signal;
      const result = checkWorkflowInterruption(signal);

      expect(result.type).toBe("continue");
    });
  });

  describe("with aborted signal (workflow pause)", () => {
    it("should return paused when reason contains PAUSE interruption type", () => {
      // Core's checkExecutionInterruption already parses the abort reason,
      // so the mock returns the already-parsed result
      mockCheckExecutionInterruption.mockReturnValue({
        type: "paused",
        executionId: "exec-1",
        nodeId: "node-1",
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("paused");
      if (result.type === "paused") {
        expect(result.nodeId).toBe("node-1");
        expect(result.executionId).toBe("exec-1");
      }
    });

    it("should return paused with unknown nodeId when nodeId is missing", () => {
      // Core returns paused without nodeId; workflow wrapper defaults to "unknown"
      mockCheckExecutionInterruption.mockReturnValue({
        type: "paused",
        executionId: "exec-1",
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("paused");
      if (result.type === "paused") {
        expect(result.nodeId).toBe("unknown");
        expect(result.executionId).toBe("exec-1");
      }
    });
  });

  describe("with aborted signal (workflow stop)", () => {
    it("should return stopped when reason contains STOP interruption type", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "stopped",
        executionId: "exec-2",
        nodeId: "node-2",
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("stopped");
      if (result.type === "stopped") {
        expect(result.nodeId).toBe("node-2");
        expect(result.executionId).toBe("exec-2");
      }
    });

    it("should return stopped with unknown nodeId when nodeId is missing", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "stopped",
        executionId: "exec-2",
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("stopped");
      if (result.type === "stopped") {
        expect(result.nodeId).toBe("unknown");
        expect(result.executionId).toBe("exec-2");
      }
    });
  });

  describe("with aborted signal (generic abort)", () => {
    it("should return aborted when reason is not an interruption object", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "aborted",
        reason: new Error("Generic abort"),
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("aborted");
    });

    it("should return aborted when reason is undefined", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "aborted",
        reason: undefined,
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("aborted");
    });

    it("should return aborted when reason is a string", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "aborted",
        reason: "Abort reason",
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("aborted");
    });

    it("should return aborted when reason object lacks interruptionType", () => {
      mockCheckExecutionInterruption.mockReturnValue({
        type: "aborted",
        reason: { someOtherField: "value" },
      });

      const result = checkWorkflowInterruption();

      expect(result.type).toBe("aborted");
    });
  });
});

describe("getWorkflowInterruptionType", () => {
  it("should return PAUSE for paused result", () => {
    const result = { type: "paused", nodeId: "node-1" } as const;
    const interruptionType = getWorkflowInterruptionType(result);

    expect(interruptionType).toBe("PAUSE");
  });

  it("should return STOP for stopped result", () => {
    const result = { type: "stopped", nodeId: "node-1" } as const;
    const interruptionType = getWorkflowInterruptionType(result);

    expect(interruptionType).toBe("STOP");
  });

  it("should return null for continue result", () => {
    const result = { type: "continue" } as const;
    const interruptionType = getWorkflowInterruptionType(result);

    expect(interruptionType).toBeNull();
  });

  it("should return null for aborted result", () => {
    const result = { type: "aborted", reason: new Error("Aborted") } as const;
    const interruptionType = getWorkflowInterruptionType(result);

    expect(interruptionType).toBeNull();
  });
});

describe("getWorkflowInterruptionDescription", () => {
  it("should return description for continue", () => {
    const result = { type: "continue" } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Workflow execution continuing");
  });

  it("should return description for paused with nodeId", () => {
    const result = { type: "paused", nodeId: "node-123" } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Workflow execution paused at node: node-123");
  });

  it("should return description for stopped with nodeId", () => {
    const result = { type: "stopped", nodeId: "node-456" } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Workflow execution stopped at node: node-456");
  });

  it("should return description for aborted with reason", () => {
    const result = { type: "aborted", reason: "Custom abort reason" } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Custom abort reason");
  });

  it("should return default description for aborted without reason", () => {
    const result = { type: "aborted" } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Workflow execution operation aborted");
  });

  it("should return description for aborted with Error reason", () => {
    const result = { type: "aborted", reason: new Error("Error abort") } as const;
    const description = getWorkflowInterruptionDescription(result);

    expect(description).toBe("Error: Error abort");
  });
});

describe("toWorkflowInterruptionResult", () => {
  it("should add nodeId to paused result", () => {
    const baseResult = { type: "paused" as const };
    const result = toWorkflowInterruptionResult(baseResult, "node-1");

    expect(result.type).toBe("paused");
    if (result.type === "paused") {
      expect(result.nodeId).toBe("node-1");
    }
  });

  it("should add nodeId to stopped result", () => {
    const baseResult = { type: "stopped" as const };
    const result = toWorkflowInterruptionResult(baseResult, "node-2");

    expect(result.type).toBe("stopped");
    if (result.type === "stopped") {
      expect(result.nodeId).toBe("node-2");
    }
  });

  it("should return continue result unchanged", () => {
    const baseResult = { type: "continue" as const };
    const result = toWorkflowInterruptionResult(baseResult, "node-3");

    expect(result.type).toBe("continue");
  });

  it("should return aborted result unchanged", () => {
    const baseResult = { type: "aborted" as const, reason: "Aborted" };
    const result = toWorkflowInterruptionResult(baseResult, "node-4");

    expect(result.type).toBe("aborted");
  });
});

describe("createWorkflowInterruptionAbortReason", () => {
  it("should create PAUSE abort reason", () => {
    const reason = createWorkflowInterruptionAbortReason("PAUSE", "exec-1", "node-1");

    expect(reason).toBeInstanceOf(WorkflowExecutionInterruptedException);
    expect(reason.interruptionType).toBe("PAUSE");
    expect(reason.executionId).toBe("exec-1");
    expect(reason.nodeId).toBe("node-1");
    expect(reason.message).toBe("Workflow execution paused");
  });

  it("should create STOP abort reason", () => {
    const reason = createWorkflowInterruptionAbortReason("STOP", "exec-2", "node-2");

    expect(reason).toBeInstanceOf(WorkflowExecutionInterruptedException);
    expect(reason.interruptionType).toBe("STOP");
    expect(reason.executionId).toBe("exec-2");
    expect(reason.nodeId).toBe("node-2");
    expect(reason.message).toBe("Workflow execution stopped");
  });

  it("should include context in abort reason", () => {
    const reason = createWorkflowInterruptionAbortReason("PAUSE", "exec-3", "node-3");

    expect(reason.context).toBeDefined();
    expect(reason.context?.["executionId"]).toBe("exec-3");
    expect(reason.context?.["nodeId"]).toBe("node-3");
  });
});
