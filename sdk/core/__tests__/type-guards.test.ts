/**
 * Type Guards Test - Verify event type guard functions work correctly
 */

import { describe, it, expect } from "vitest";
import { generateId } from "@wf-agent/common-utils";
import type { 
  Event,
  NodeStartedEvent,
  NodeFailedEvent,
  CheckpointCreatedEvent,
  ToolCallStartedEvent,
  AgentCustomEvent,
  WorkflowExecutionStartedEvent 
} from "@wf-agent/types";
import {
  isNodeEvent,
  isCheckpointEvent,
  isToolEvent,
  isWorkflowExecutionEvent,
  isAgentCustomEvent,
  isErrorEvent,
  isCompletionEvent,
} from "@wf-agent/types";

describe("Event Type Guards", () => {

  // ============================================================================
  // Category-Based Guards Tests
  // ============================================================================

  describe("isNodeEvent", () => {
    it("should correctly identify node events", () => {
      const nodeStartedEvent: NodeStartedEvent = {
        id: generateId(),
        type: "NODE_STARTED",
        timestamp: Date.now(),
        nodeId: "node-1",
        nodeType: "agent-loop",
      };

      expect(isNodeEvent(nodeStartedEvent)).toBe(true);
      if (isNodeEvent(nodeStartedEvent)) {
        expect(nodeStartedEvent.nodeId).toBe("node-1");
        expect(nodeStartedEvent.nodeType).toBe("agent-loop");
      }
    });

    it("should return false for non-node events", () => {
      const workflowEvent: WorkflowExecutionStartedEvent = {
        id: generateId(),
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: Date.now(),
        workflowId: "wf-1",
        executionId: "exec-1",
        input: {},
      };

      expect(isNodeEvent(workflowEvent)).toBe(false);
    });
  });

  describe("isCheckpointEvent", () => {
    it("should correctly identify checkpoint events", () => {
      const checkpointEvent: CheckpointCreatedEvent = {
        id: generateId(),
        type: "CHECKPOINT_CREATED",
        timestamp: Date.now(),
        checkpointId: "checkpoint-1",
        description: "Test checkpoint",
      };

      expect(isCheckpointEvent(checkpointEvent)).toBe(true);
      if (isCheckpointEvent(checkpointEvent)) {
        expect(checkpointEvent.checkpointId).toBe("checkpoint-1");
      }
    });

    it("should return false for non-checkpoint events", () => {
      const workflowEvent: WorkflowExecutionStartedEvent = {
        id: generateId(),
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: Date.now(),
        workflowId: "wf-1",
        executionId: "exec-1",
        input: {},
      };

      expect(isCheckpointEvent(workflowEvent)).toBe(false);
    });
  });

  describe("isToolEvent", () => {
    it("should correctly identify tool events", () => {
      const toolEvent: ToolCallStartedEvent = {
        id: generateId(),
        type: "TOOL_CALL_STARTED",
        timestamp: Date.now(),
        nodeId: "node-1",
        toolId: "tool-1",
        toolArguments: '{}',
      };

      expect(isToolEvent(toolEvent)).toBe(true);
      if (isToolEvent(toolEvent)) {
        expect(toolEvent.toolId).toBe("tool-1");
        expect(toolEvent.nodeId).toBe("node-1");
      }
    });

    it("should return false for non-tool events", () => {
      const workflowEvent: WorkflowExecutionStartedEvent = {
        id: generateId(),
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: Date.now(),
        workflowId: "wf-1",
        executionId: "exec-1",
        input: {},
      };

      expect(isToolEvent(workflowEvent)).toBe(false);
    });
  });

  describe("isWorkflowExecutionEvent", () => {
    it("should correctly identify workflow execution events", () => {
      const workflowEvent: WorkflowExecutionStartedEvent = {
        id: generateId(),
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: Date.now(),
        workflowId: "wf-1",
        executionId: "exec-1",
        input: {},
      };

      expect(isWorkflowExecutionEvent(workflowEvent)).toBe(true);
      if (isWorkflowExecutionEvent(workflowEvent)) {
        expect(workflowEvent.workflowId).toBe("wf-1");
        expect(workflowEvent.input).toEqual({});
      }
    });

    it("should return false for non-workflow events", () => {
      const nodeEvent: NodeStartedEvent = {
        id: generateId(),
        type: "NODE_STARTED",
        timestamp: Date.now(),
        nodeId: "node-1",
        nodeType: "agent-loop",
      };

      expect(isWorkflowExecutionEvent(nodeEvent)).toBe(false);
    });
  });

  describe("isAgentCustomEvent", () => {
    it("should correctly identify agent custom events", () => {
      const agentEvent: AgentCustomEvent = {
        id: generateId(),
        type: "AGENT_CUSTOM_EVENT",
        timestamp: Date.now(),
        agentLoopId: "agent-1",
        eventName: "custom-event",
        eventData: { key: "value" },
      };

      expect(isAgentCustomEvent(agentEvent)).toBe(true);
      if (isAgentCustomEvent(agentEvent)) {
        expect(agentEvent.agentLoopId).toBe("agent-1");
        expect(agentEvent.eventName).toBe("custom-event");
        expect(agentEvent.eventData).toEqual({ key: "value" });
      }
    });

    it("should return false for non-agent events", () => {
      const workflowEvent: WorkflowExecutionStartedEvent = {
        id: generateId(),
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: Date.now(),
        workflowId: "wf-1",
        executionId: "exec-1",
        input: {},
      };

      expect(isAgentCustomEvent(workflowEvent)).toBe(false);
    });
  });

  describe("isErrorEvent", () => {
    it("should correctly identify error events", () => {
      const failedEvent: NodeFailedEvent = {
        id: generateId(),
        type: "NODE_FAILED",
        timestamp: Date.now(),
        nodeId: "node-1",
        error: new Error("Test error"),
      };

      expect(isErrorEvent(failedEvent)).toBe(true);
      if (isErrorEvent(failedEvent)) {
        expect(failedEvent.error).toBeDefined();
      }
    });

    it("should return false for non-error events", () => {
      const completedEvent: Event = {
        id: generateId(),
        type: "NODE_COMPLETED",
        timestamp: Date.now(),
        nodeId: "node-1",
        output: {},
        executionTime: 1000,
      };

      expect(isErrorEvent(completedEvent)).toBe(false);
    });
  });

  describe("isCompletionEvent", () => {
    it("should correctly identify completion events", () => {
      const completedEvent: Event = {
        id: generateId(),
        type: "NODE_COMPLETED",
        timestamp: Date.now(),
        nodeId: "node-1",
        output: { result: "success" },
        executionTime: 1000,
      };

      expect(isCompletionEvent(completedEvent)).toBe(true);
      if (isCompletionEvent(completedEvent)) {
        expect(completedEvent.output).toEqual({ result: "success" });
        expect(completedEvent.executionTime).toBe(1000);
      }
    });

    it("should return false for non-completion events", () => {
      const startedEvent: NodeStartedEvent = {
        id: generateId(),
        type: "NODE_STARTED",
        timestamp: Date.now(),
        nodeId: "node-1",
        nodeType: "agent-loop",
      };

      expect(isCompletionEvent(startedEvent)).toBe(false);
    });
  });
});
