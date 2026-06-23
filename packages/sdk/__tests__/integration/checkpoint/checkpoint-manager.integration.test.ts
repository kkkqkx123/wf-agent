/**
 * CheckpointManager Integration Tests
 *
 * Tests the end-to-end checkpoint creation, restoration, and querying flows
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { CheckpointManager } from "../checkpoint-manager.js";
import { LayertwineExecutor } from "../../services/executors/remote/implementations/layertwine/index.js";
import type { AgentStateSnapshot, GraphStateSnapshot } from "../types.js";
import type { AgentLoopStateSnapshot } from "@wf-agent/types";
import type { WorkflowExecutionStateSnapshot } from "@wf-agent/types";

describe("CheckpointManager Integration", () => {
  let checkpointManager: CheckpointManager;
  let executor: LayertwineExecutor;

  beforeAll(async () => {
    executor = new LayertwineExecutor({
      deployMode: "remote",
      address: "localhost:5000",
    });

    try {
      await executor.connect({
        address: "localhost:5000",
        useTls: false,
        timeout: 30000,
      });
    } catch (error) {
      console.warn("Layertwine service not available, skipping integration tests");
      return;
    }

    checkpointManager = new CheckpointManager(executor);
  });

  afterAll(async () => {
    if (executor.isConnected()) {
      await executor.disconnect();
    }
  });

  it("should create and restore agent checkpoint", async () => {
    const agentLoopState: AgentLoopStateSnapshot = {
      status: "COMPLETED",
      currentIteration: 5,
      toolCallCount: 3,
      startTime: Date.now() - 10000,
      endTime: Date.now(),
      error: null,
    };

    const snapshot: AgentStateSnapshot = {
      agentLoopId: "test-agent-1",
      messages: [
        {
          id: "msg-1",
          role: "user",
          content: "Test message",
        },
      ] as never[],
      state: agentLoopState,
      timestamp: Date.now(),
    };

    const checkpointId = await checkpointManager.createAgentCheckpoint(
      snapshot,
      "Test agent checkpoint"
    );

    expect(checkpointId).toBeTruthy();

    const restored = await checkpointManager.restoreAgentState(checkpointId);

    expect(restored.agentLoopId).toBe(snapshot.agentLoopId);
    expect(restored.state.currentIteration).toBe(5);
    expect(restored.messages.length).toBe(1);
  });

  it("should create and restore graph checkpoint", async () => {
    const graphState: WorkflowExecutionStateSnapshot = {
      status: "COMPLETED",
      currentNodeId: "node-1",
      variables: [],
      variableState: {
        globalVariables: {},
        executionVariables: {},
        temporaryVariables: {},
      },
      input: { test: "input" },
      output: { result: "output" },
      nodeResults: {},
      errors: [],
      conversationState: {
        messages: [],
        markMap: new Map(),
        tokenUsage: null,
        currentRequestUsage: null,
      },
    };

    const snapshot: GraphStateSnapshot = {
      executionId: "exec-1",
      workflowId: "workflow-1",
      state: graphState,
      timestamp: Date.now(),
    };

    const checkpointId = await checkpointManager.createGraphCheckpoint(
      snapshot,
      "Test graph checkpoint"
    );

    expect(checkpointId).toBeTruthy();

    const restored = await checkpointManager.restoreGraphState(checkpointId);

    expect(restored.executionId).toBe(snapshot.executionId);
    expect(restored.workflowId).toBe(snapshot.workflowId);
  });

  it("should list checkpoints", async () => {
    const checkpoints = await checkpointManager.listCheckpoints();

    expect(Array.isArray(checkpoints)).toBe(true);
    if (checkpoints.length > 0) {
      expect(checkpoints[0]).toHaveProperty("id");
      expect(checkpoints[0]).toHaveProperty("createdAt");
    }
  });
});
