/**
 * Test for Iteration Type Cleanup Changes
 * 
 * This test verifies that:
 * 1. AgentIterationData has correct fields (no status field)
 * 2. ITERATION_START event type exists
 * 3. AgentLoopStateSnapshot includes currentIterationRecord
 * 
 * Note: Iteration status is conveyed via message/event types, not a status field.
 */

import { describe, it, expect } from "vitest";
import { AgentStreamEventType } from "@wf-agent/types";
import type { 
  AgentIterationData, 
  IterationStartEvent,
  IterationCompleteEvent,
  AgentLoopStateSnapshot 
} from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("Iteration Type Cleanup", () => {
  it("should have correct AgentIterationData structure", () => {
    // This is a type-checking test - if it compiles, the types are correct
    const iterationData: AgentIterationData = {
      iteration: 1,
      toolCallCount: 2,
      duration: 1500,
    };

    expect(iterationData.iteration).toBe(1);
    expect(iterationData.toolCallCount).toBe(2);
    expect(iterationData.duration).toBe(1500);
    
    // Verify old fields are removed (this should cause compile error if they exist)
    // @ts-expect-error - status should not exist
    expect(iterationData.status).toBeUndefined();
    // @ts-expect-error - maxIterations should not exist
    expect(iterationData.maxIterations).toBeUndefined();
    // @ts-expect-error - messageCount should not exist  
    expect(iterationData.messageCount).toBeUndefined();
  });

  it("should have ITERATION_START event type", () => {
    expect(AgentStreamEventType.ITERATION_START).toBe("iteration_start");
    expect(AgentStreamEventType.ITERATION_COMPLETE).toBe("iteration_complete");
  });

  it("should support IterationStartEvent type", () => {
    const startEvent: IterationStartEvent = {
      type: AgentStreamEventType.ITERATION_START,
      timestamp: Date.now(),
      agentLoopId: "test-agent-1",
      iteration: 1,
    };

    expect(startEvent.type).toBe("iteration_start");
    expect(startEvent.iteration).toBe(1);
  });

  it("should support IterationCompleteEvent type", () => {
    const completeEvent: IterationCompleteEvent = {
      type: AgentStreamEventType.ITERATION_COMPLETE,
      timestamp: Date.now(),
      agentLoopId: "test-agent-1",
      iteration: 1,
      shouldContinue: true,
    };

    expect(completeEvent.type).toBe("iteration_complete");
    expect(completeEvent.shouldContinue).toBe(true);
  });

  it("should include currentIterationRecord in AgentLoopStateSnapshot", () => {
    const snapshot: AgentLoopStateSnapshot = {
      status: AgentLoopStatus.RUNNING,
      currentIteration: 2,
      toolCallCount: 3,
      startTime: Date.now() - 10000,
      endTime: null,
      error: null,
      messages: [],
      iterationHistory: [
        {
          iteration: 1,
          startTime: Date.now() - 5000,
          endTime: Date.now() - 4000,
          toolCalls: [],
          responseContent: "First response",
        },
      ],
      currentIterationRecord: {
        iteration: 2,
        startTime: Date.now() - 1000,
        toolCalls: [
          {
            id: "tool-call-1",
            name: "test_tool",
            arguments: { param: "value" },
            startTime: Date.now() - 500,
          },
        ],
        responseContent: undefined,
      },
    };

    expect(snapshot.currentIteration).toBe(2);
    expect(snapshot.iterationHistory).toHaveLength(1);
    expect(snapshot.currentIterationRecord).toBeDefined();
    expect(snapshot.currentIterationRecord?.iteration).toBe(2);
    expect(snapshot.currentIterationRecord?.toolCalls).toHaveLength(1);
  });

  it("should support optional toolCallCount and duration fields", () => {
    // Started iteration may not have toolCallCount or duration yet
    const started: AgentIterationData = {
      iteration: 1,
    };

    // Completed iteration has toolCallCount and duration
    const completed: AgentIterationData = {
      iteration: 1,
      toolCallCount: 2,
      duration: 1500,
    };

    expect(started.iteration).toBe(1);
    expect(started.toolCallCount).toBeUndefined();
    expect(started.duration).toBeUndefined();
    
    expect(completed.iteration).toBe(1);
    expect(completed.toolCallCount).toBe(2);
    expect(completed.duration).toBe(1500);
  });
});
