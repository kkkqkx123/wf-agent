/**
 * Tests for EventMetricsCollector label aggregation
 */

import { describe, it, expect } from "vitest";
import { EventMetricsCollector } from "../event-collector.js";

describe("EventMetricsCollector - Label Aggregation", () => {
  describe("getStatisticsByLabel", () => {
    it("should aggregate events by workflow_id", () => {
      const collector = new EventMetricsCollector();

      // Record events for different workflows
      collector.recordEvent("NODE_COMPLETED", "exec-1", {
        workflow_id: "wf-123",
      });
      collector.recordEvent("NODE_COMPLETED", "exec-2", {
        workflow_id: "wf-123",
      });
      collector.recordEvent("NODE_STARTED", "exec-3", {
        workflow_id: "wf-456",
      });
      collector.recordEvent("NODE_STARTED", "exec-4", {
        workflow_id: "wf-456",
      });
      collector.recordEvent("NODE_COMPLETED", "exec-5", {
        workflow_id: "wf-456",
      });

      const stats = collector.getStatisticsByLabel("workflow_id");

      expect(stats["wf-123"]).toBe(2);
      expect(stats["wf-456"]).toBe(3);
      expect(Object.keys(stats).length).toBe(2);
    });

    it("should aggregate events by agent_loop_id", () => {
      const collector = new EventMetricsCollector();

      // Record events for different agent loops
      collector.recordEvent("AGENT_TURN_COMPLETED", "exec-1", {
        agent_loop_id: "agent-123",
      });
      collector.recordEvent("AGENT_TURN_COMPLETED", "exec-2", {
        agent_loop_id: "agent-123",
      });
      collector.recordEvent("AGENT_TURN_COMPLETED", "exec-3", {
        agent_loop_id: "agent-456",
      });

      const stats = collector.getStatisticsByLabel("agent_loop_id");

      expect(stats["agent-123"]).toBe(2);
      expect(stats["agent-456"]).toBe(1);
    });

    it("should handle events without specified label", () => {
      const collector = new EventMetricsCollector();

      // Record some events with label
      collector.recordEvent("NODE_COMPLETED", "exec-1", {
        workflow_id: "wf-123",
      });

      // Record some events without label
      collector.recordEvent("ERROR", "exec-2");

      const stats = collector.getStatisticsByLabel("workflow_id");

      expect(stats["wf-123"]).toBe(1);
      // Events without the label should be excluded
      expect(Object.keys(stats).length).toBe(1);
    });

    it("should filter by event type when provided", () => {
      const collector = new EventMetricsCollector();

      // Record multiple event types for same workflow
      collector.recordEvent("NODE_COMPLETED", "exec-1", {
        workflow_id: "wf-123",
      });
      collector.recordEvent("NODE_STARTED", "exec-2", {
        workflow_id: "wf-123",
      });
      collector.recordEvent("TOOL_CALL_COMPLETED", "exec-3", {
        workflow_id: "wf-123",
      });

      // Query only NODE_COMPLETED events
      const stats = collector.getStatisticsByLabel("workflow_id", [
        "NODE_COMPLETED",
      ]);

      expect(stats["wf-123"]).toBe(1);
    });

    it("should return empty object when no matching labels found", () => {
      const collector = new EventMetricsCollector();

      collector.recordEvent("NODE_COMPLETED", "exec-1", {
        workflow_id: "wf-123",
      });

      // Query for non-existent label
      const stats = collector.getStatisticsByLabel("non_existent_label");

      expect(stats).toEqual({});
    });

    it("should handle concurrent workflow and agent loop events", () => {
      const collector = new EventMetricsCollector();

      // Simulate concurrent execution with both workflow and agent events
      collector.recordEvent("WORKFLOW_EXECUTION_STARTED", "exec-1", {
        workflow_id: "wf-123",
      });
      collector.recordEvent("AGENT_STARTED", "exec-1", {
        workflow_id: "wf-123",
        agent_loop_id: "agent-123",
      });
      collector.recordEvent("AGENT_TURN_COMPLETED", "exec-1", {
        workflow_id: "wf-123",
        agent_loop_id: "agent-123",
      });

      const workflowStats = collector.getStatisticsByLabel("workflow_id");
      const agentStats = collector.getStatisticsByLabel("agent_loop_id");

      expect(workflowStats["wf-123"]).toBe(3);
      expect(agentStats["agent-123"]).toBe(2);
    });
  });
});
