/**
 * Agent Loop Checkpoint Serializer Tests
 *
 * Tests serialization and deserialization of agent loop checkpoints.
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  AgentLoopCheckpointSerializer,
  AgentLoopCheckpointDeltaCalculator,
} from "../entities/agent-loop-checkpoint-serializer.js";
import type { AgentLoopCheckpoint, AgentLoopStateSnapshot } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopCheckpointSerializer", () => {
  let serializer: AgentLoopCheckpointSerializer;

  beforeEach(() => {
    serializer = new AgentLoopCheckpointSerializer();
  });

  const createTestCheckpoint = (type: "FULL" | "DELTA"): AgentLoopCheckpoint => {
    if (type === "FULL") {
      const snapshot: AgentLoopStateSnapshot = {
        status: AgentLoopStatus.RUNNING,
        currentIteration: 5,
        toolCallCount: 10,
        startTime: Date.now() - 10000,
        endTime: null,
        error: null,
        messages: [
          { role: "user", content: "Hello" },
          { role: "assistant", content: "Hi there!" },
        ],
        variables: { count: 42, name: "test" },
        iterationHistory: [],
      };
      return {
        id: "test-checkpoint-1",
        agentLoopId: "test-agent-loop-1",
        timestamp: Date.now(),
        type: "FULL",
        snapshot,
        metadata: {
          description: "Test checkpoint",
          tags: ["test"],
        },
      };
    } else {
      return {
        id: "test-checkpoint-1",
        agentLoopId: "test-agent-loop-1",
        timestamp: Date.now(),
        type: "DELTA",
        baseCheckpointId: "base-checkpoint-1",
        previousCheckpointId: "prev-checkpoint-1",
        delta: {
          addedMessages: [{ role: "user", content: "New message" }],
          modifiedVariables: new Map([["count", 43]]),
        },
        metadata: {
          description: "Test checkpoint",
          tags: ["test"],
        },
      };
    }
  };

  it("should serialize and deserialize a FULL checkpoint", async () => {
    const checkpoint = createTestCheckpoint("FULL");

    // Serialize
    const serialized = await serializer.serializeCheckpoint(checkpoint);
    expect(serialized).toBeInstanceOf(Uint8Array);
    expect(serialized.length).toBeGreaterThan(0);

    // Deserialize
    const deserialized = await serializer.deserializeCheckpoint(serialized);

    // Verify
    expect(deserialized.id).toBe(checkpoint.id);
    expect(deserialized.agentLoopId).toBe(checkpoint.agentLoopId);
    expect(deserialized.type).toBe("FULL");
    expect(deserialized.timestamp).toBe(checkpoint.timestamp);

    if (deserialized.type === "FULL" && checkpoint.type === "FULL") {
      expect(deserialized.snapshot.status).toBe(checkpoint.snapshot.status);
      expect(deserialized.snapshot.currentIteration).toBe(checkpoint.snapshot.currentIteration);
      expect(deserialized.snapshot.toolCallCount).toBe(checkpoint.snapshot.toolCallCount);
      expect(deserialized.snapshot.messages.length).toBe(checkpoint.snapshot.messages.length);
    }
  });

  it("should serialize and deserialize a DELTA checkpoint", async () => {
    const checkpoint = createTestCheckpoint("DELTA");

    // Serialize
    const serialized = await serializer.serializeCheckpoint(checkpoint);
    expect(serialized).toBeInstanceOf(Uint8Array);

    // Deserialize
    const deserialized = await serializer.deserializeCheckpoint(serialized);

    // Verify
    expect(deserialized.id).toBe(checkpoint.id);
    expect(deserialized.type).toBe("DELTA");

    if (deserialized.type === "DELTA" && checkpoint.type === "DELTA") {
      expect(deserialized.baseCheckpointId).toBe(checkpoint.baseCheckpointId);
      expect(deserialized.previousCheckpointId).toBe(checkpoint.previousCheckpointId);
      expect(deserialized.delta).toBeDefined();
    }
  });

  it("should throw error when deserializing wrong entity type", async () => {
    // Create a different serializer and serialize with wrong type
    const { WorkflowCheckpointSerializer } = await import(
      "../entities/checkpoint-serializer.js"
    );
    const workflowSerializer = new WorkflowCheckpointSerializer();

    // This would be a workflow checkpoint, not an agent loop checkpoint
    const wrongCheckpoint = {
      id: "test-checkpoint-1",
      executionId: "test-execution-1",
      workflowId: "test-workflow-1",
      timestamp: Date.now(),
      type: "FULL" as const,
      snapshot: {
        status: "RUNNING" as any,
        currentNodeId: "node-1",
        variables: [],
        variableScopes: {} as any,
        input: {},
        output: {},
        nodeResults: {},
        errors: [],
        conversationState: {
          messages: [],
          markMap: {} as any,
          tokenUsage: null,
          currentRequestUsage: null,
        },
      },
    };

    const serialized = await workflowSerializer.serializeCheckpoint(wrongCheckpoint as any);

    // Should throw when trying to deserialize as agent loop checkpoint
    await expect(serializer.deserializeCheckpoint(serialized)).rejects.toThrow(
      "Expected agentLoopCheckpoint",
    );
  });
});

describe("AgentLoopCheckpointDeltaCalculator", () => {
  let calculator: AgentLoopCheckpointDeltaCalculator;

  beforeEach(() => {
    calculator = new AgentLoopCheckpointDeltaCalculator();
  });

  it("should return FULL when no previous checkpoint", () => {
    const current = {
      id: "current-1",
      agentLoopId: "agent-1",
      timestamp: Date.now(),
      type: "FULL" as const,
      snapshot: {
        status: AgentLoopStatus.RUNNING,
        currentIteration: 1,
        toolCallCount: 0,
        startTime: Date.now(),
        endTime: null,
        error: null,
        messages: [],
        variables: {},
      },
    };

    const result = calculator.calculateCheckpointDelta(null, current);
    expect(result.type).toBe("FULL");
    expect(result.snapshot).toBe(current);
  });

  it("should calculate delta between two checkpoints", () => {
    const previous: AgentLoopCheckpoint = {
      id: "prev-1",
      agentLoopId: "agent-1",
      timestamp: Date.now() - 1000,
      type: "FULL" as const,
      snapshot: {
        status: AgentLoopStatus.RUNNING,
        currentIteration: 1,
        toolCallCount: 0,
        startTime: Date.now() - 2000,
        endTime: null,
        error: null,
        messages: [{ role: "user", content: "Hello" }],
        variables: { count: 1 },
      },
    };

    const current: AgentLoopCheckpoint = {
      id: "current-1",
      agentLoopId: "agent-1",
      timestamp: Date.now(),
      type: "FULL" as const,
      snapshot: {
        status: AgentLoopStatus.RUNNING,
        currentIteration: 2,
        toolCallCount: 1,
        startTime: Date.now() - 2000,
        endTime: null,
        error: null,
        messages: [
          { role: "user", content: "Hello" },
          { role: "assistant", content: "Hi!" },
        ],
        variables: { count: 2 },
      },
    };

    const result = calculator.calculateCheckpointDelta(previous, current);
    expect(result.type).toBe("DELTA");
    expect(result.delta).toBeDefined();

    if (result.type === "DELTA" && result.delta) {
      expect(result.delta.addedMessages).toBeDefined();
      expect(result.delta.modifiedVariables).toBeDefined();
    }
  });
});
