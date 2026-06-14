/**
 * Unit tests for agent-loop-lifecycle.ts
 *
 * Tests checkpoint creation, resource cleanup, and entity cloning
 * for the Agent Loop lifecycle.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { AgentLoopEntity as AgentLoopEntityType } from "../../../entities/agent-loop-entity.js";

const {
  mockConversationManagerRef,
  mockSetParentContextRef,
  mockGetParentContextRef,
  MockAgentLoopEntity,
  mockCreateCheckpoint,
} = vi.hoisted(() => {
  interface MockConversationManager {
    createSnapshot: ReturnType<typeof vi.fn>;
    restoreFromSnapshot: ReturnType<typeof vi.fn>;
  }

  const mockConversationManagerRef: MockConversationManager = {
    createSnapshot: vi.fn(),
    restoreFromSnapshot: vi.fn(),
  };
  const mockSetParentContextRef: (context: object) => void = vi.fn();
  const mockGetParentContextRef: () => object | undefined = vi.fn(() => undefined);

  interface MockEntityInstance {
    id: string;
    config: unknown;
    state: unknown;
    conversationManager: MockConversationManager;
    getConversationManager: () => MockConversationManager;
    setConversationManager: (cm: MockConversationManager) => void;
    setParentContext: (context: object) => void;
    getParentContext: () => object | undefined;
    cleanup: () => void;
    getStatus: () => string;
  }

  const MockAgentLoopEntity = vi.fn().mockImplementation(function (
    this: MockEntityInstance,
    id: string,
    config: unknown,
    state: unknown,
  ) {
    this.id = id;
    this.config = config;
    this.state = state;
    this.conversationManager = mockConversationManagerRef;
    this.getConversationManager = vi.fn(() => mockConversationManagerRef);
    this.setConversationManager = vi.fn();
    this.setParentContext = mockSetParentContextRef;
    this.getParentContext = mockGetParentContextRef;
    this.cleanup = vi.fn();
    this.getStatus = vi.fn(() => "RUNNING");
  });

  const mockCreateCheckpoint = vi.fn().mockResolvedValue("checkpoint-1");

  return {
    mockConversationManagerRef,
    mockSetParentContextRef,
    mockGetParentContextRef,
    MockAgentLoopEntity,
    mockCreateCheckpoint,
  };
});

// Mock external dependencies
vi.mock("../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../../../checkpoint/index.js", () => ({
  AgentLoopCheckpointCoordinator: vi.fn().mockImplementation(function () {
    return { createCheckpoint: mockCreateCheckpoint };
  }),
}));

vi.mock("../../../entities/agent-loop-entity.js", () => ({
  AgentLoopEntity: MockAgentLoopEntity,
}));

// Import after mocks
import {
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopCheckpointDependencies,
} from "../agent-loop-lifecycle.js";
import { AgentStateCoordinator } from "../../../state-managers/agent-state-coordinator.js";

describe("AgentLoopLifecycle", () => {
  let mockEntity: AgentLoopEntityType;

  beforeEach(() => {
    vi.clearAllMocks();

    mockConversationManagerRef.createSnapshot = vi.fn(() => ({
      messages: [],
      markMap: {
        originalIndices: [],
        batchBoundaries: [],
        boundaryToBatch: [],
        currentBatch: 0,
      },
    }));
    mockConversationManagerRef.restoreFromSnapshot = vi.fn();
    (mockSetParentContextRef as unknown as ReturnType<typeof vi.fn>).mockReset();
    (mockGetParentContextRef as unknown as ReturnType<typeof vi.fn>).mockReset();
    (mockGetParentContextRef as unknown as ReturnType<typeof vi.fn>).mockReturnValue(undefined);

    mockEntity = {
      id: "agent-loop-1",
      config: { profileId: "test-profile" },
      state: {
        currentIteration: 3,
        toolCallCount: 5,
        status: "RUNNING",
        clone: vi.fn(() => ({
          currentIteration: 3,
          toolCallCount: 5,
          status: "RUNNING",
        })),
      },
      conversationManager: mockConversationManagerRef,
      getConversationManager: vi.fn(() => mockConversationManagerRef),
      setConversationManager: vi.fn(),
      getParentContext: mockGetParentContextRef,
      getStatus: vi.fn(() => "RUNNING"),
      cleanup: vi.fn(),
    } as unknown as AgentLoopEntityType;
  });

  describe("createAgentLoopCheckpoint", () => {
    it("should create a checkpoint and return its ID", async () => {
      const dependencies: AgentLoopCheckpointDependencies = {
        saveCheckpoint: vi.fn().mockResolvedValue("cp-1"),
        getCheckpoint: vi.fn(),
        listCheckpoints: vi.fn().mockResolvedValue([]),
      };

      const result = await createAgentLoopCheckpoint(mockEntity, dependencies);

      expect(result).toBe("checkpoint-1");
      expect(mockCreateCheckpoint).toHaveBeenCalledTimes(1);
    });

    it("should pass metadata options to the coordinator", async () => {
      const dependencies: AgentLoopCheckpointDependencies = {
        saveCheckpoint: vi.fn().mockResolvedValue("cp-1"),
        getCheckpoint: vi.fn(),
        listCheckpoints: vi.fn().mockResolvedValue([]),
      };

      await createAgentLoopCheckpoint(mockEntity, dependencies, {
        description: "test checkpoint",
        tags: ["test", "iteration-3"],
        metadata: { version: "1.0" },
      });

      expect(mockCreateCheckpoint).toHaveBeenCalledWith(
        mockEntity,
        dependencies,
        expect.objectContaining({
          description: "test checkpoint",
          tags: ["test", "iteration-3"],
        }),
      );
    });
  });

  describe("cleanupAgentLoop", () => {
    it("should call entity.cleanup()", () => {
      cleanupAgentLoop(mockEntity);

      expect(mockEntity.cleanup).toHaveBeenCalledTimes(1);
    });
  });

  describe("cloneAgentLoop", () => {
    let mockStateCoordinator: AgentStateCoordinator;

    beforeEach(() => {
      mockStateCoordinator = {
        createSnapshot: vi.fn().mockReturnValue({
          messages: [],
          markMap: {
            originalIndices: [],
            batchBoundaries: [],
            boundaryToBatch: [],
            currentBatch: 0,
          },
        }),
        restoreFromSnapshot: vi.fn(),
      } as unknown as AgentStateCoordinator;
    });

    it("should create a new AgentLoopEntity with cloned state", () => {
      const result = cloneAgentLoop(mockEntity, mockStateCoordinator);
      const cloned = result.entity;

      expect(cloned).toBeInstanceOf(Object);
      expect(cloned.id).toBe("agent-loop-1");
      expect(cloned.config).toEqual(mockEntity.config);
      expect(MockAgentLoopEntity).toHaveBeenCalledWith(
        "agent-loop-1",
        { profileId: "test-profile" },
        mockEntity.state.clone(),
      );
    });

    it("should clone conversation manager snapshot", () => {
      const snapshot = { messages: [{ role: "user", content: "hello" }] };
      mockStateCoordinator.createSnapshot = vi.fn(() => snapshot);

      cloneAgentLoop(mockEntity, mockStateCoordinator);

      expect(mockStateCoordinator.createSnapshot).toHaveBeenCalled();
    });

    it("should set parent context if entity has one", () => {
      const parentContext = { parentType: "WORKFLOW" as const, parentId: "wf-1", nodeId: "node-1" };
      (mockGetParentContextRef as unknown as ReturnType<typeof vi.fn>).mockReturnValue(
        parentContext,
      );

      const result = cloneAgentLoop(mockEntity, mockStateCoordinator);
      const cloned = result.entity;

      expect(cloned.getParentContext?.()).toEqual(parentContext);
    });

    it("should not set parent context when entity has none", () => {
      (mockGetParentContextRef as unknown as ReturnType<typeof vi.fn>).mockReturnValue(undefined);

      const result = cloneAgentLoop(mockEntity, mockStateCoordinator);
      const cloned = result.entity;

      expect(cloned.getParentContext?.()).toBeUndefined();
    });
  });
});
