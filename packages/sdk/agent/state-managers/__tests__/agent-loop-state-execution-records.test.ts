/**
 * Unit tests for AgentLoopState execution records (Plan C)
 *
 * Tests the new execution record tracking functionality:
 * - Error records
 * - Interruption records
 * - Event records
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopState } from "../agent-loop-state.js";
import {
  type ExecutionErrorRecord,
  type ExecutionInterruptionRecord,
  type ExecutionEventRecord,
  EXECUTION_STATE_MAX_ERROR_RECORDS,
  EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
  EXECUTION_STATE_MAX_EVENTS,
} from "@wf-agent/types";

describe("AgentLoopState - Execution Records (Plan C)", () => {
  let state: AgentLoopState;

  beforeEach(() => {
    state = new AgentLoopState();
  });

  describe("Error Records", () => {
    it("should add error records", () => {
      const error: ExecutionErrorRecord = {
        id: "error-1",
        timestamp: Date.now(),
        message: "Test error",
        severity: "error",
        iteration: 1,
        nodeId: "node-1",
        context: {
          operation: "tool_execution",
          toolName: "test_tool",
        },
        isRecoverable: true,
        recoveryAction: "retry",
      };

      state.addErrorRecord(error);
      const records = state.getErrorRecords();

      expect(records).toHaveLength(1);
      expect(records[0]).toEqual(error);
    });

    it("should limit error records to prevent bloat", () => {
      // Add more records than the limit
      for (let i = 0; i < EXECUTION_STATE_MAX_ERROR_RECORDS + 10; i++) {
        const error: ExecutionErrorRecord = {
          id: `error-${i}`,
          timestamp: Date.now(),
          message: `Error ${i}`,
          severity: "error",
          iteration: i,
          context: {
            operation: "test",
          },
          isRecoverable: false,
        };
        state.addErrorRecord(error);
      }

      const records = state.getErrorRecords();
      // [P5 Fix] Error records are no longer capped - all records are retained
      expect(records).toHaveLength(110);

      // Check that all records are kept (first and last)
      expect(records[0]!.id).toBe(
        `error-0`,
      );
      expect(records[109]!.id).toBe(
        `error-109`,
      );
    });

    it("should include error records in snapshot", () => {
      const error: ExecutionErrorRecord = {
        id: "error-1",
        timestamp: Date.now(),
        message: "Test error",
        severity: "warning",
        context: {
          operation: "llm_streaming",
        },
        isRecoverable: false,
      };

      state.addErrorRecord(error);
      const snapshot = state.createSnapshot();

      expect(snapshot.errorRecords).toBeDefined();
      expect(snapshot.errorRecords).toHaveLength(1);
      expect(snapshot.errorRecords![0]).toEqual(error);
    });

    it("should restore error records from snapshot", () => {
      const errors: ExecutionErrorRecord[] = [
        {
          id: "error-1",
          timestamp: Date.now(),
          message: "Error 1",
          severity: "error",
          context: { operation: "test" },
          isRecoverable: true,
        },
        {
          id: "error-2",
          timestamp: Date.now(),
          message: "Error 2",
          severity: "warning",
          context: { operation: "test" },
          isRecoverable: false,
        },
      ];

      state.addErrorRecord(errors[0]!);
      state.addErrorRecord(errors[1]!);
      const snapshot = state.createSnapshot();

      const newState = new AgentLoopState();
      newState.restoreFromSnapshot(snapshot);

      const restoredErrors = newState.getErrorRecords();
      expect(restoredErrors).toHaveLength(2);
      expect(restoredErrors).toEqual(errors);
    });
  });

  describe("Interruption Records", () => {
    it("should add interruption records", () => {
      const interruption: ExecutionInterruptionRecord = {
        id: "int-1",
        timestamp: Date.now(),
        type: "PAUSE",
        reason: "User requested pause",
        iteration: 2,
        nodeId: "node-2",
        status: "pending",
      };

      state.addInterruptionRecord(interruption);
      const records = state.getInterruptionRecords();

      expect(records).toHaveLength(1);
      expect(records[0]).toEqual(interruption);
    });

    it("should limit interruption records to prevent bloat", () => {
      // Add more records than the limit
      for (let i = 0; i < EXECUTION_STATE_MAX_INTERRUPTION_RECORDS + 5; i++) {
        const interruption: ExecutionInterruptionRecord = {
          id: `int-${i}`,
          timestamp: Date.now(),
          type: "PAUSE",
          reason: `Pause ${i}`,
          iteration: i,
          status: "pending",
        };
        state.addInterruptionRecord(interruption);
      }

      const records = state.getInterruptionRecords();
      expect(records).toHaveLength(EXECUTION_STATE_MAX_INTERRUPTION_RECORDS);
    });

    it("should include interruption records in snapshot", () => {
      const interruption: ExecutionInterruptionRecord = {
        id: "int-1",
        timestamp: Date.now(),
        type: "STOP",
        reason: "Max iterations reached",
        iteration: 5,
        status: "resumed",
        checkpointId: "ckpt-1",
        resumedAt: Date.now() + 1000,
        resumedFromCheckpointId: "ckpt-1",
      };

      state.addInterruptionRecord(interruption);
      const snapshot = state.createSnapshot();

      expect(snapshot.interruptionRecords).toBeDefined();
      expect(snapshot.interruptionRecords).toHaveLength(1);
      expect(snapshot.interruptionRecords![0]).toEqual(interruption);
    });

    it("should restore interruption records from snapshot", () => {
      const interruptions: ExecutionInterruptionRecord[] = [
        {
          id: "int-1",
          timestamp: Date.now(),
          type: "PAUSE",
          reason: "Pause 1",
          iteration: 1,
          status: "pending",
        },
        {
          id: "int-2",
          timestamp: Date.now(),
          type: "STOP",
          reason: "Stop 1",
          iteration: 2,
          status: "resumed",
        },
      ];

      state.addInterruptionRecord(interruptions[0]!);
      state.addInterruptionRecord(interruptions[1]!);
      const snapshot = state.createSnapshot();

      const newState = new AgentLoopState();
      newState.restoreFromSnapshot(snapshot);

      const restoredInterruptions = newState.getInterruptionRecords();
      expect(restoredInterruptions).toHaveLength(2);
      expect(restoredInterruptions).toEqual(interruptions);
    });
  });

  describe("Event Records", () => {
    it("should add event records", () => {
      const event: ExecutionEventRecord = {
        id: "evt-1",
        timestamp: Date.now(),
        type: "tool_executed",
        iteration: 1,
        nodeId: "node-1",
        message: "Tool execution completed",
        severity: "info",
        data: {
          toolName: "test_tool",
          status: "success",
        },
      };

      state.addEventRecord(event);
      const records = state.getEventRecords();

      expect(records).toHaveLength(1);
      expect(records[0]).toEqual(event);
    });

    it("should limit event records to prevent bloat", () => {
      // Add more records than the limit
      for (let i = 0; i < EXECUTION_STATE_MAX_EVENTS + 10; i++) {
        const event: ExecutionEventRecord = {
          id: `evt-${i}`,
          timestamp: Date.now(),
          type: "state_changed",
          iteration: i,
          message: `Event ${i}`,
        };
        state.addEventRecord(event);
      }

      const records = state.getEventRecords();
      expect(records).toHaveLength(EXECUTION_STATE_MAX_EVENTS);
    });

    it("should include event records in snapshot", () => {
      const events: ExecutionEventRecord[] = [
        {
          id: "evt-1",
          timestamp: Date.now(),
          type: "iteration_started",
          iteration: 1,
          message: "Iteration 1 started",
          severity: "info",
        },
        {
          id: "evt-2",
          timestamp: Date.now(),
          type: "iteration_completed",
          iteration: 1,
          message: "Iteration 1 completed",
          severity: "info",
        },
      ];

      state.addEventRecord(events[0]!);
      state.addEventRecord(events[1]!);
      const snapshot = state.createSnapshot();

      expect(snapshot.eventRecords).toBeDefined();
      expect(snapshot.eventRecords).toHaveLength(2);
      expect(snapshot.eventRecords).toEqual(events);
    });

    it("should restore event records from snapshot", () => {
      const events: ExecutionEventRecord[] = [
        {
          id: "evt-1",
          timestamp: Date.now(),
          type: "state_changed",
          message: "State changed",
        },
        {
          id: "evt-2",
          timestamp: Date.now(),
          type: "error_occurred",
          message: "Error occurred",
          severity: "error",
        },
      ];

      state.addEventRecord(events[0]!);
      state.addEventRecord(events[1]!);
      const snapshot = state.createSnapshot();

      const newState = new AgentLoopState();
      newState.restoreFromSnapshot(snapshot);

      const restoredEvents = newState.getEventRecords();
      expect(restoredEvents).toHaveLength(2);
      expect(restoredEvents).toEqual(events);
    });
  });

  describe("Snapshot with mixed records", () => {
    it("should include all record types in snapshot", () => {
      const error: ExecutionErrorRecord = {
        id: "error-1",
        timestamp: Date.now(),
        message: "Test error",
        severity: "error",
        context: { operation: "test" },
        isRecoverable: true,
      };

      const interruption: ExecutionInterruptionRecord = {
        id: "int-1",
        timestamp: Date.now(),
        type: "PAUSE",
        reason: "Pause",
        iteration: 1,
        status: "pending",
      };

      const event: ExecutionEventRecord = {
        id: "evt-1",
        timestamp: Date.now(),
        type: "state_changed",
        message: "State changed",
      };

      state.addErrorRecord(error);
      state.addInterruptionRecord(interruption);
      state.addEventRecord(event);

      const snapshot = state.createSnapshot();

      expect(snapshot.errorRecords).toHaveLength(1);
      expect(snapshot.interruptionRecords).toHaveLength(1);
      expect(snapshot.eventRecords).toHaveLength(1);
    });

    it("should restore all record types from snapshot", () => {
      const error: ExecutionErrorRecord = {
        id: "error-1",
        timestamp: Date.now(),
        message: "Test error",
        severity: "error",
        context: { operation: "test" },
        isRecoverable: true,
      };

      const interruption: ExecutionInterruptionRecord = {
        id: "int-1",
        timestamp: Date.now(),
        type: "PAUSE",
        reason: "Pause",
        iteration: 1,
        status: "pending",
      };

      const event: ExecutionEventRecord = {
        id: "evt-1",
        timestamp: Date.now(),
        type: "state_changed",
        message: "State changed",
      };

      state.addErrorRecord(error);
      state.addInterruptionRecord(interruption);
      state.addEventRecord(event);

      const snapshot = state.createSnapshot();

      const newState = new AgentLoopState();
      newState.restoreFromSnapshot(snapshot);

      expect(newState.getErrorRecords()).toEqual([error]);
      expect(newState.getInterruptionRecords()).toEqual([interruption]);
      expect(newState.getEventRecords()).toEqual([event]);
    });
  });

  describe("Cleanup and reset", () => {
    it("should clear records on cleanup", () => {
      state.addErrorRecord({
        id: "error-1",
        timestamp: Date.now(),
        message: "Error",
        severity: "error",
        context: { operation: "test" },
        isRecoverable: false,
      });

      state.addInterruptionRecord({
        id: "int-1",
        timestamp: Date.now(),
        type: "PAUSE",
        reason: "Pause",
        iteration: 1,
        status: "pending",
      });

      state.addEventRecord({
        id: "evt-1",
        timestamp: Date.now(),
        type: "state_changed",
        message: "Event",
      });

      state.cleanup();

      expect(state.getErrorRecords()).toHaveLength(0);
      expect(state.getInterruptionRecords()).toHaveLength(0);
      expect(state.getEventRecords()).toHaveLength(0);
    });

    it("should clear records on reset", () => {
      state.addErrorRecord({
        id: "error-1",
        timestamp: Date.now(),
        message: "Error",
        severity: "error",
        context: { operation: "test" },
        isRecoverable: false,
      });

      state.addInterruptionRecord({
        id: "int-1",
        timestamp: Date.now(),
        type: "PAUSE",
        reason: "Pause",
        iteration: 1,
        status: "pending",
      });

      state.addEventRecord({
        id: "evt-1",
        timestamp: Date.now(),
        type: "state_changed",
        message: "Event",
      });

      state.reset();

      expect(state.getErrorRecords()).toHaveLength(0);
      expect(state.getInterruptionRecords()).toHaveLength(0);
      expect(state.getEventRecords()).toHaveLength(0);
    });
  });
});
