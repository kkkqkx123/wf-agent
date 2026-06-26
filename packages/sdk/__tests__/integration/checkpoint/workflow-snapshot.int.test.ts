/**
 * Workflow Snapshot Integration Tests
 *
 * Tests WorkflowExecutionStateSnapshot serialization and restoration.
 * Covers: CP-INT-21 through CP-INT-25
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { StateCodec } from "@wf-agent/common-utils";
import type {
  WorkflowExecutionStateSnapshot,
  LLMMessage,
  MessageMarkMap,
  TokenUsage,
  NodeExecutionResult,
  CheckpointVariableState,
  TriggerRuntimeState,
  ExecutionHierarchyMetadata,
  ForkJoinAggregationState,
  HookExecutionContext,
} from "@wf-agent/types";

function createTestSnapshot(overrides?: Partial<WorkflowExecutionStateSnapshot>): WorkflowExecutionStateSnapshot {
  return {
    status: "RUNNING",
    currentNodeId: "node-1",
    variables: [],
    variableState: {
      globalVariables: {},
      executionVariables: {},
      temporaryVariables: {},
    },
    input: { test: "input" },
    output: null,
    nodeResults: {},
    errors: [],
    conversationState: {
      messages: [],
      markMap: {
        currentBatch: 0,
        batchBoundaries: [0],
        originalIndices: [],
        boundaryToBatch: [],
      },
      tokenUsage: null,
      currentRequestUsage: null,
    },
    ...overrides,
  } as WorkflowExecutionStateSnapshot;
}

function createTestMessage(id: string, role: "user" | "assistant" | "system", content: string): LLMMessage {
  return {
    id,
    role,
    content,
    timestamp: Date.now(),
  } as LLMMessage;
}

function createTestTokenUsage(): TokenUsage {
  return {
    totalTokens: 1000,
    promptTokens: 600,
    completionTokens: 400,
  } as TokenUsage;
}

function createTestNodeResult(nodeId: string): NodeExecutionResult {
  return {
    nodeId,
    status: "COMPLETED",
    startTime: Date.now() - 10000,
    endTime: Date.now(),
    output: { result: "success" },
    error: null,
  } as NodeExecutionResult;
}

describe("Workflow Snapshot Integration", () => {
  let codec: StateCodec;

  beforeEach(() => {
    codec = new StateCodec();
  });

  describe("CP-INT-21: snapshot serialization", () => {
    it("should serialize and deserialize basic snapshot", async () => {
      const snapshot = createTestSnapshot();
      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.status).toBe(snapshot.status);
      expect(deserialized.currentNodeId).toBe(snapshot.currentNodeId);
      expect(deserialized.input).toEqual(snapshot.input);
    });

    it("should serialize and deserialize snapshot with messages", async () => {
      const messages: LLMMessage[] = [
        createTestMessage("msg-1", "user", "Hello"),
        createTestMessage("msg-2", "assistant", "Hi there!"),
        createTestMessage("msg-3", "user", "How are you?"),
      ];

      const snapshot = createTestSnapshot({
        conversationState: {
          messages,
          markMap: {
            currentBatch: 1,
            batchBoundaries: [0, 2],
            originalIndices: [0, 1, 2],
            boundaryToBatch: [0, 0, 1],
          } as MessageMarkMap,
          tokenUsage: createTestTokenUsage(),
          currentRequestUsage: createTestTokenUsage(),
        },
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.conversationState.messages).toHaveLength(3);
      expect(deserialized.conversationState.messages[0].content).toBe("Hello");
      expect(deserialized.conversationState.tokenUsage.totalTokens).toBe(1000);
    });

    it("should serialize and deserialize snapshot with node results", async () => {
      const nodeResults: Record<string, NodeExecutionResult> = {
        "node-1": createTestNodeResult("node-1"),
        "node-2": createTestNodeResult("node-2"),
      };

      const snapshot = createTestSnapshot({ nodeResults });
      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(Object.keys(deserialized.nodeResults)).toHaveLength(2);
      expect(deserialized.nodeResults["node-1"].status).toBe("COMPLETED");
    });

    it("should serialize and deserialize snapshot with errors", async () => {
      const snapshot = createTestSnapshot({
        status: "FAILED",
        errors: [
          { code: "ERR_001", message: "Something went wrong", timestamp: Date.now() } as any,
        ],
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.status).toBe("FAILED");
      expect(deserialized.errors).toHaveLength(1);
    });
  });

  describe("CP-INT-22: variable state serialization", () => {
    it("should serialize and deserialize variable state", async () => {
      const variableState: CheckpointVariableState = {
        variables: {
          "var-1": { name: "var-1", type: "string", value: "hello", readonly: false },
          "var-2": { name: "var-2", type: "number", value: 42, readonly: false },
          "var-3": { name: "var-3", type: "boolean", value: true, readonly: false },
        },
      };

      const snapshot = createTestSnapshot({
        variables: [
          { name: "var-1", type: "string", value: "hello", readonly: false },
          { name: "var-2", type: "number", value: 42, readonly: false },
        ],
        variableState,
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.variableState.variables["var-1"].value).toBe("hello");
      expect(deserialized.variableState.variables["var-2"].value).toBe(42);
      expect(deserialized.variableState.variables["var-3"].value).toBe(true);
    });
  });

  describe("CP-INT-23: trigger state serialization", () => {
    it("should serialize and deserialize trigger states", async () => {
      const triggerStates = new Map<string, TriggerRuntimeState>();
      triggerStates.set("trigger-1", {
        triggerId: "trigger-1",
        status: "COMPLETED",
        firedAt: Date.now(),
        output: { data: "test" },
      } as TriggerRuntimeState);

      const snapshot = createTestSnapshot({
        triggerStates,
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.triggerStates).toBeDefined();
    });
  });

  describe("CP-INT-24: hierarchy metadata serialization", () => {
    it("should serialize and deserialize hierarchy metadata", async () => {
      const hierarchy: ExecutionHierarchyMetadata = {
        parentId: "parent-wf-1",
        parentType: "WORKFLOW",
        children: [
          { childId: "child-1", childType: "WORKFLOW", forkPathId: "path-a" },
          { childId: "child-2", childType: "AGENT_LOOP" },
        ],
      };

      const snapshot = createTestSnapshot({ hierarchy });
      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.hierarchy).toBeDefined();
      expect(deserialized.hierarchy!.children).toHaveLength(2);
    });
  });

  describe("CP-INT-25: FORK/JOIN aggregation state serialization", () => {
    it("should serialize and deserialize fork join aggregation state", async () => {
      const forkJoinState: ForkJoinAggregationState = {
        forkNodeId: "fork-1",
        joinNodeId: "join-1",
        totalPaths: 3,
        completedPaths: new Set(["path-a", "path-b"]),
        pendingPaths: new Set(["path-c"]),
        failedPaths: new Set(),
        isAggregationComplete: false,
      };

      const snapshot = createTestSnapshot({
        forkJoinAggregationState: forkJoinState,
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.forkJoinAggregationState).toBeDefined();
      expect(deserialized.forkJoinAggregationState!.forkNodeId).toBe("fork-1");
    });

    it("should serialize and deserialize hook execution context", async () => {
      const hookContext: HookExecutionContext = {
        workflowInput: { test: "input" },
        output: { result: "output" },
        variables: { var1: "value1" },
        messages: [
          createTestMessage("msg-1", "user", "Hello"),
        ],
      };

      const snapshot = createTestSnapshot({
        hookExecutionContext: hookContext,
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.hookExecutionContext).toBeDefined();
      expect(deserialized.hookExecutionContext!.messages).toHaveLength(1);
    });
  });

  describe("CP-INT-26: edge cases", () => {
    it("should handle empty snapshot", async () => {
      const snapshot = createTestSnapshot({
        conversationState: {
          messages: [],
          markMap: {
            currentBatch: 0,
            batchBoundaries: [0],
            originalIndices: [],
            boundaryToBatch: [],
          },
          tokenUsage: null,
          currentRequestUsage: null,
        },
        nodeResults: {},
        variables: [],
        variableState: { globalVariables: {}, executionVariables: {}, temporaryVariables: {} },
        errors: [],
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.conversationState.messages).toHaveLength(0);
      expect(Object.keys(deserialized.nodeResults)).toHaveLength(0);
    });

    it("should handle large message arrays", async () => {
      const messages: LLMMessage[] = [];
      for (let i = 0; i < 100; i++) {
        messages.push(createTestMessage(`msg-${i}`, i % 2 === 0 ? "user" : "assistant", `Message content ${i}`));
      }

      const snapshot = createTestSnapshot({
        conversationState: {
          messages,
          markMap: {
            currentBatch: 10,
            batchBoundaries: [0, 25, 50, 75],
            originalIndices: Array.from({ length: 100 }, (_, i) => i),
            boundaryToBatch: Array.from({ length: 100 }, (_, i) => Math.floor(i / 25)),
          } as MessageMarkMap,
          tokenUsage: createTestTokenUsage(),
          currentRequestUsage: createTestTokenUsage(),
        },
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.conversationState.messages).toHaveLength(100);
    });

    it("should handle null and undefined values", async () => {
      const snapshot = createTestSnapshot({
        output: null,
        triggerStates: undefined,
        forkJoinAggregationState: undefined,
        hookExecutionContext: undefined,
      });

      const serialized = await codec.serialize(snapshot);
      const deserialized = await codec.deserialize<WorkflowExecutionStateSnapshot>(serialized);

      expect(deserialized.output).toBeNull();
      expect(deserialized.triggerStates).toBeUndefined();
    });
  });
});
