/**
 * AgentLoopCoordinator Unit Tests
 *
 * Tests for the Agent Loop lifecycle management:
 * - buildEntity: entity creation with interruption state
 * - execute: full sync execution lifecycle
 * - executeStream: full stream execution lifecycle
 * - start: async execution
 * - pause/resume/continue/stop: lifecycle management
 * - get/getStatus/getRunning/getPaused: query operations
 * - cleanup/destroy: resource management
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentLoopRuntimeConfig, AgentLoopResult } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import type { AgentLoopRegistry } from "../../../stores/agent-loop-registry.js";
import type { AgentLoopExecutor } from "../../executors/agent-loop-executor.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { AgentLoopMetricsCollector } from "../../../../metrics/agent-loop-collector.js";
import { AgentLoopCoordinator } from "../agent-loop-coordinator.js";
import type { AgentLoopStreamEvent } from "../agent-execution-coordinator.js";

// Mock AgentLoopFactory
vi.mock("../../factories/agent-loop-factory.js", () => ({
  AgentLoopFactory: {
    create: vi.fn(),
    fromCheckpoint: vi.fn(),
  },
}));

describe("AgentLoopCoordinator", () => {
  let coordinator: AgentLoopCoordinator;
  let mockRegistry: AgentLoopRegistry;
  let mockExecutor: AgentLoopExecutor;
  let mockGlobalContext: GlobalContext;
  let mockEventManager: EventRegistry;
  let mockMetricsCollector: AgentLoopMetricsCollector;
  let mockEntity: any;
  let mockInterruptionManager: any;

  const defaultConfig: AgentLoopRuntimeConfig = {
    agentConfigId: "agent-config-1",
    profileId: "profile-1",
    maxIterations: 10,
    availableTools: [],
  } as unknown as AgentLoopRuntimeConfig;

  const defaultResult: AgentLoopResult = {
    success: true,
    iterations: 3,
    toolCallCount: 2,
    content: "Final answer",
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockInterruptionManager = {
      create: vi.fn().mockReturnValue({
        setEventRegistry: vi.fn(),
        connectToParent: vi.fn(),
      }),
    };

    mockEntity = {
      id: "agent-loop-1",
      config: { ...defaultConfig },
      state: {
        currentIteration: 0,
        toolCallCount: 0,
        pendingToolCalls: new Set<string>(),
        isStreaming: false,
        streamMessage: null,
        start: vi.fn(),
        pause: vi.fn(),
        resume: vi.fn(),
        complete: vi.fn(),
        fail: vi.fn(),
        cancel: vi.fn(),
        recordToolCallStart: vi.fn(),
        recordToolCallEnd: vi.fn(),
      },
      conversationManager: {
        getMessageCount: vi.fn().mockReturnValue(0),
        getMessages: vi.fn().mockReturnValue([]),
      },
      getStatus: vi.fn().mockReturnValue(AgentLoopStatus.CREATED),
      isRunning: vi.fn().mockReturnValue(true),
      isPaused: vi.fn().mockReturnValue(false),
      shouldPause: vi.fn().mockReturnValue(false),
      shouldStop: vi.fn().mockReturnValue(false),
      getMessages: vi.fn().mockReturnValue([]),
      setInterruptionState: vi.fn(),
      interrupt: vi.fn(),
      resetInterrupt: vi.fn(),
      cleanup: vi.fn(),
    };

    const mockStateCoordinator = {
      getMessageCount: vi.fn().mockReturnValue(0),
      getMessages: vi.fn().mockReturnValue([]),
      getConversationManager: vi.fn().mockReturnValue({
        getAllMessages: vi.fn().mockReturnValue([]),
      }),
    };

    mockRegistry = {
      register: vi.fn(),
      unregister: vi.fn(),
      get: vi.fn().mockResolvedValue(mockEntity),
      getRunning: vi.fn().mockReturnValue([]),
      getPaused: vi.fn().mockReturnValue([]),
      cleanupTerminated: vi.fn().mockReturnValue(0),
      clear: vi.fn(),
      registerStateCoordinator: vi.fn(),
      getStateCoordinator: vi.fn().mockReturnValue(mockStateCoordinator),
    } as unknown as AgentLoopRegistry;

    mockExecutor = {
      execute: vi.fn().mockResolvedValue(defaultResult),
      executeStream: vi.fn(),
      setEventEmitter: vi.fn(),
      setEventManager: vi.fn(),
    } as unknown as AgentLoopExecutor;

    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
      cleanupExecutionListeners: vi.fn().mockReturnValue(0),
    } as unknown as EventRegistry;

    mockGlobalContext = {
      container: {
        get: vi.fn().mockReturnValue(mockInterruptionManager),
      },
      eventRegistry: mockEventManager,
    } as unknown as GlobalContext;

    mockMetricsCollector = {
      recordExecutionStart: vi.fn(),
      recordExecutionComplete: vi.fn(),
      recordError: vi.fn(),
      recordPause: vi.fn(),
      recordResume: vi.fn(),
    } as unknown as AgentLoopMetricsCollector;
  });

  describe("execute", () => {
    it("should create entity, execute and complete successfully", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const result = await coordinator.execute(defaultConfig);

      expect(result.success).toBe(true);
      expect(result.content).toBe("Final answer");
      expect(mockRegistry.register).toHaveBeenCalledWith(mockEntity);
      expect(mockExecutor.execute).toHaveBeenCalledWith(mockEntity, expect.any(Object));
    });

    it("should emit events via stateTransitor on success", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockEntity.getStatus
        .mockReturnValueOnce(AgentLoopStatus.CREATED) // before start
        .mockReturnValueOnce(AgentLoopStatus.RUNNING) // after start
        .mockReturnValueOnce(AgentLoopStatus.RUNNING); // after execution

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await coordinator.execute(defaultConfig);

      // Should emit start and complete events via state transitor
      expect(mockEventManager.emit).toHaveBeenCalled();
    });

    it("should handle execution failure", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockExecutor.execute = vi.fn().mockResolvedValue({
        success: false,
        iterations: 1,
        toolCallCount: 0,
        error: "Tool execution failed",
      });

      mockEntity.getStatus
        .mockReturnValueOnce(AgentLoopStatus.CREATED)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
        mockMetricsCollector,
      );

      const result = await coordinator.execute(defaultConfig);

      expect(result.success).toBe(false);
      expect(result.error).toBe("Tool execution failed");
    });

    it("should handle exceptions during execution", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockExecutor.execute = vi.fn().mockRejectedValue(new Error("Unexpected error"));

      mockEntity.getStatus
        .mockReturnValueOnce(AgentLoopStatus.CREATED)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
        mockMetricsCollector,
      );

      const result = await coordinator.execute(defaultConfig);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(mockMetricsCollector.recordError).toHaveBeenCalled();
    });

    it("should establish interruption cascade when parentExecutionId is provided", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const interruptionManager = {
        setEventRegistry: vi.fn(),
        connectToParent: vi.fn(),
      };
      mockGlobalContext.container.get = vi.fn().mockReturnValue({
        create: vi.fn().mockReturnValue(interruptionManager),
      });

      await coordinator.execute(defaultConfig, { parentExecutionId: "workflow-1" });

      expect(interruptionManager.setEventRegistry).toHaveBeenCalledWith(mockEventManager);
      expect(interruptionManager.connectToParent).toHaveBeenCalledWith("workflow-1");
    });

    it("should not re-complete if entity already completed internally", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      // Entity already COMPLETED (e.g., by executeIteration internally)
      mockEntity.getStatus.mockReturnValue(AgentLoopStatus.COMPLETED);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await coordinator.execute(defaultConfig);

      // Should not throw state machine violation
      expect(mockEventManager.emit).toHaveBeenCalled();
    });
  });

  describe("executeStream", () => {
    it("should stream events and complete", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      const mockStream = (async function* (): AsyncGenerator<AgentLoopStreamEvent> {
        yield { type: "agent_start", agentLoopId: "agent-loop-1" } as AgentLoopStreamEvent;
        yield {
          type: "iteration_complete",
          agentLoopId: "agent-loop-1",
          iteration: 1,
        } as unknown as AgentLoopStreamEvent;
      })();

      mockExecutor.executeStream = vi.fn().mockReturnValue(mockStream);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const events: AgentLoopStreamEvent[] = [];
      for await (const event of coordinator.executeStream(defaultConfig)) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
      expect(mockEventManager.cleanupExecutionListeners).toHaveBeenCalledWith("agent-loop-1");
    });

    it("should handle pause during stream execution", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockEntity.shouldPause = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);

      const mockStream = (async function* (): AsyncGenerator<AgentLoopStreamEvent> {
        yield { type: "agent_start", agentLoopId: "agent-loop-1" } as AgentLoopStreamEvent;
        yield { type: "iteration_complete", agentLoopId: "agent-loop-1", iteration: 1 } as any;
      })();

      mockExecutor.executeStream = vi.fn().mockReturnValue(mockStream);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const events: AgentLoopStreamEvent[] = [];
      for await (const event of coordinator.executeStream(defaultConfig)) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
    });

    it("should handle stop during stream execution", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockEntity.shouldStop = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);

      const mockStream = (async function* (): AsyncGenerator<AgentLoopStreamEvent> {
        yield { type: "agent_start", agentLoopId: "agent-loop-1" } as AgentLoopStreamEvent;
        yield { type: "iteration_complete", agentLoopId: "agent-loop-1", iteration: 1 } as any;
      })();

      mockExecutor.executeStream = vi.fn().mockReturnValue(mockStream);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const events: AgentLoopStreamEvent[] = [];
      for await (const event of coordinator.executeStream(defaultConfig)) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
    });

    it("should handle errors during stream execution", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      mockExecutor.executeStream = vi.fn().mockImplementation(async function* () {
        throw new Error("Stream execution error");
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(async () => {
        const events: AgentLoopStreamEvent[] = [];
        for await (const event of coordinator.executeStream(defaultConfig)) {
          events.push(event);
        }
      }).rejects.toThrow("Stream execution error");
    });
  });

  describe("start (async execution)", () => {
    it("should return entity ID immediately", async () => {
      const { AgentLoopFactory } = await import("../../factories/agent-loop-factory.js");
      (AgentLoopFactory.create as ReturnType<typeof vi.fn>).mockResolvedValue({
        entity: mockEntity,
        stateCoordinator: mockRegistry.getStateCoordinator("agent-loop-1"),
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const id = await coordinator.start(defaultConfig);

      expect(id).toBe("agent-loop-1");
      expect(mockRegistry.register).toHaveBeenCalled();
      expect(mockExecutor.execute).toHaveBeenCalled();
    });
  });

  describe("pause", () => {
    it("should pause a running agent loop", async () => {
      mockEntity.isRunning.mockReturnValue(true);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await coordinator.pause("agent-loop-1");

      expect(mockEntity.interrupt).toHaveBeenCalledWith("PAUSE");
    });

    it("should throw if entity not found", async () => {
      mockRegistry.get = vi.fn().mockResolvedValue(undefined);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.pause("nonexistent")).rejects.toThrow("AgentLoop not found");
    });

    it("should throw if entity is not running", async () => {
      mockEntity.isRunning.mockReturnValue(false);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.pause("agent-loop-1")).rejects.toThrow("AgentLoop is not running");
    });
  });

  describe("resume", () => {
    it("should resume a paused agent loop", async () => {
      mockEntity.isPaused.mockReturnValue(true);
      mockEntity.isRunning.mockReturnValue(false);
      mockEntity.getStatus
        .mockReturnValueOnce(AgentLoopStatus.PAUSED)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
        mockMetricsCollector,
      );

      const result = await coordinator.resume("agent-loop-1");

      expect(result.success).toBe(true);
      expect(mockEntity.resetInterrupt).toHaveBeenCalled();
      expect(mockExecutor.execute).toHaveBeenCalled();
      expect(mockMetricsCollector.recordResume).toHaveBeenCalled();
    });

    it("should throw if entity not found", async () => {
      mockRegistry.get = vi.fn().mockResolvedValue(undefined);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.resume("nonexistent")).rejects.toThrow("AgentLoop not found");
    });

    it("should throw if entity is not paused", async () => {
      mockEntity.isPaused.mockReturnValue(false);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.resume("agent-loop-1")).rejects.toThrow("AgentLoop is not paused");
    });
  });

  describe("continue", () => {
    it("should continue execution when last message is not assistant", async () => {
      mockEntity.isRunning.mockReturnValue(false);
      mockRegistry.getStateCoordinator = vi.fn().mockReturnValue({
        getMessages: vi.fn().mockReturnValue([{ role: "user", content: "Continue please" }]),
        getMessageCount: vi.fn().mockReturnValue(1),
        getConversationManager: vi.fn().mockReturnValue({
          getAllMessages: vi.fn().mockReturnValue([]),
        }),
      });
      mockEntity.getStatus
        .mockReturnValueOnce(AgentLoopStatus.CREATED)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING)
        .mockReturnValueOnce(AgentLoopStatus.RUNNING);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const result = await coordinator.continue("agent-loop-1");

      expect(result.success).toBe(true);
      expect(mockExecutor.execute).toHaveBeenCalledWith(mockEntity, expect.any(Object));
    });

    it("should throw if entity is running", async () => {
      mockEntity.isRunning.mockReturnValue(true);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.continue("agent-loop-1")).rejects.toThrow(
        "AgentLoop is currently running",
      );
    });

    it("should throw if last message is assistant", async () => {
      mockEntity.isRunning.mockReturnValue(false);
      mockRegistry.getStateCoordinator = vi.fn().mockReturnValue({
        getMessages: vi.fn().mockReturnValue([
          { role: "assistant", content: "Here is the answer" },
        ]),
        getMessageCount: vi.fn().mockReturnValue(1),
        getConversationManager: vi.fn().mockReturnValue({
          getAllMessages: vi.fn().mockReturnValue([]),
        }),
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.continue("agent-loop-1")).rejects.toThrow(
        "last message must be user or toolResult",
      );
    });

    it("should throw if entity has no messages", async () => {
      mockEntity.isRunning.mockReturnValue(false);
      mockRegistry.getStateCoordinator = vi.fn().mockReturnValue({
        getMessages: vi.fn().mockReturnValue([]),
        getMessageCount: vi.fn().mockReturnValue(0),
        getConversationManager: vi.fn().mockReturnValue({
          getAllMessages: vi.fn().mockReturnValue([]),
        }),
      });

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.continue("agent-loop-1")).rejects.toThrow(
        "AgentLoop has no messages",
      );
    });

    it("should throw if entity not found", async () => {
      mockRegistry.get = vi.fn().mockResolvedValue(undefined);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.continue("nonexistent")).rejects.toThrow("AgentLoop not found");
    });
  });

  describe("stop", () => {
    it("should stop a running agent loop", async () => {
      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await coordinator.stop("agent-loop-1");

      expect(mockEntity.interrupt).toHaveBeenCalledWith("STOP");
      expect(mockEntity.cleanup).toHaveBeenCalled();
      expect(mockRegistry.unregister).toHaveBeenCalledWith("agent-loop-1");
      expect(mockEventManager.cleanupExecutionListeners).toHaveBeenCalledWith("agent-loop-1");
    });

    it("should throw if entity not found", async () => {
      mockRegistry.get = vi.fn().mockResolvedValue(undefined);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      await expect(coordinator.stop("nonexistent")).rejects.toThrow("AgentLoop not found");
    });
  });

  describe("query operations", () => {
    it("should get entity by id", async () => {
      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const result = await coordinator.get("agent-loop-1");
      expect(result).toBe(mockEntity);
      expect(mockRegistry.get).toHaveBeenCalledWith("agent-loop-1");
    });

    it("should get entity status", async () => {
      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const status = await coordinator.getStatus("agent-loop-1");
      expect(status).toBe(AgentLoopStatus.CREATED);
      expect(mockEntity.getStatus).toHaveBeenCalled();
    });

    it("should return undefined status for unknown entity", async () => {
      mockRegistry.get = vi.fn().mockResolvedValue(undefined);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const status = await coordinator.getStatus("nonexistent");
      expect(status).toBeUndefined();
    });

    it("should get running instances", () => {
      const runningEntities = [mockEntity];
      mockRegistry.getRunning = vi.fn().mockReturnValue(runningEntities);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const result = coordinator.getRunning();
      expect(result).toEqual(runningEntities);
    });

    it("should get paused instances", () => {
      const pausedEntities = [mockEntity];
      mockRegistry.getPaused = vi.fn().mockReturnValue(pausedEntities);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const result = coordinator.getPaused();
      expect(result).toEqual(pausedEntities);
    });
  });

  describe("cleanup", () => {
    it("should cleanup terminated entities", () => {
      mockRegistry.cleanupTerminated = vi.fn().mockReturnValue(5);

      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      const count = coordinator.cleanup();
      expect(count).toBe(5);
    });
  });

  describe("destroy", () => {
    it("should clear the registry", () => {
      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      coordinator.destroy();
      expect(mockRegistry.clear).toHaveBeenCalled();
    });
  });

  describe("buildEntity", () => {
    it("should throw for checkpoint restore (not yet implemented)", async () => {
      coordinator = new AgentLoopCoordinator(
        mockRegistry,
        mockExecutor,
        mockGlobalContext,
        mockEventManager,
      );

      // Access private method via any cast
      await expect(
        (coordinator as any).buildEntity(defaultConfig, { checkpointId: "cp-1" }),
      ).rejects.toThrow("not yet implemented");
    });
  });
});
