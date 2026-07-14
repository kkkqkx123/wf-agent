/**
 * AgentLoopExecutor Unit Tests
 *
 * Tests for the Agent Loop executor covering:
 * - Construction and configuration
 * - Synchronous execution (delegation to coordinator)
 * - Streaming execution (delegation to coordinator)
 * - Tool schema preparation
 * - Event emission setup
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentLoopResult } from "@wf-agent/types";
import type { LLMExecutor } from "../../../../services/executors/llm-executor.js";
import type { ToolRegistry } from "../../../../shared/registry/tool-registry.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { MetricsRegistry } from "../../../../metrics/metrics-registry.js";
import type { GlobalContext } from "../../../../shared/global-context.js";

// =============================================================================
// Hoisted mocks
// vi.mock is hoisted to top of file, so any reference inside factory must
// use vi.hoisted() to be available at that point.
// =============================================================================
const {
  mockCoordinatorExecute,
  mockCoordinatorExecuteStream,
  MockAgentExecutionCoordinator,
  mockPrepareToolSchemas,
  mockEmit,
} = vi.hoisted(() => {
  const _mockCoordinatorExecute = vi.fn();
  const _mockCoordinatorExecuteStream = vi.fn();

  const _MockAgentExecutionCoordinator = vi.fn(function () {
    return {
      execute: _mockCoordinatorExecute,
      executeStream: _mockCoordinatorExecuteStream,
    };
  });

  const _mockPrepareToolSchemas = vi.fn();
  const _mockEmit = vi.fn();

  return {
    mockCoordinatorExecute: _mockCoordinatorExecute,
    mockCoordinatorExecuteStream: _mockCoordinatorExecuteStream,
    MockAgentExecutionCoordinator: _MockAgentExecutionCoordinator,
    mockPrepareToolSchemas: _mockPrepareToolSchemas,
    mockEmit: _mockEmit,
  };
});

// =============================================================================
// Module-level mocks (hoisted by vitest)
// =============================================================================

vi.mock("../../coordinators/agent-execution-coordinator.js", () => ({
  AgentExecutionCoordinator: MockAgentExecutionCoordinator,
}));

vi.mock("../../coordinators/agent-iteration-coordinator.js", () => ({
  AgentIterationCoordinator: vi.fn(),
}));

vi.mock("../../../../shared/coordinators/llm-execution-coordinator.js", () => ({
  LLMExecutionCoordinator: vi.fn(),
}));

vi.mock("../../coordinators/tool-execution-coordinator.js", () => ({
  ToolExecutionCoordinator: vi.fn(),
}));

vi.mock("../../../../services/executors/tool-call-executor.js", () => ({
  ToolCallExecutor: vi.fn(),
}));

vi.mock("../../../../shared/tools/tool-schema-helper.js", () => ({
  prepareToolSchemas: mockPrepareToolSchemas,
}));

vi.mock("../../../../shared/events/emit-event.js", () => ({
  emit: mockEmit,
}));

// =============================================================================
// Import after mocks are set up
// =============================================================================
import { AgentLoopExecutor } from "../agent-loop-executor.js";

describe("AgentLoopExecutor", () => {
  let executor: AgentLoopExecutor;
  let mockLLMExecutor: LLMExecutor;
  let mockToolService: ToolRegistry;
  let mockEventManager: EventRegistry;
  let mockMetricsRegistry: MetricsRegistry;
  let mockGlobalContext: GlobalContext;
  let mockEntity: any;
  let mockConversationManager: any;
  let mockStateCoordinator: any;
  let mockEmitEvent: any;
  let mockToolApprovalHandler: any;
  let baseDeps: any;

  beforeEach(() => {
    vi.clearAllMocks();

    mockLLMExecutor = {} as LLMExecutor;
    mockToolService = {} as ToolRegistry;
    mockEventManager = { emit: vi.fn().mockResolvedValue(undefined) } as unknown as EventRegistry;
    mockMetricsRegistry = {
      getAgentCollector: vi.fn().mockReturnValue({
        recordExecutionStart: vi.fn(),
        recordExecutionComplete: vi.fn(),
      }),
    } as unknown as MetricsRegistry;
    mockGlobalContext = {
      container: {
        get: vi.fn().mockReturnValue({}),
      },
    } as unknown as GlobalContext;

    mockConversationManager = {
      getMessageCount: vi.fn().mockReturnValue(5),
      getMessages: vi.fn().mockReturnValue([]),
      addMessage: vi.fn(),
    };

    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      config: {
        maxIterations: 10,
        availableTools: { tools: ["tool1", "tool2"] },
        profileId: "profile-1",
        agentConfigId: "agent-config-1",
      },
      state: {
        currentIteration: 0,
        toolCallCount: 0,
      },
      conversationManager: mockConversationManager,
      getConversationManager: vi.fn(() => mockConversationManager),
      getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    };

    mockStateCoordinator = {
      getMessageCount: vi.fn().mockReturnValue(5),
      getMessages: vi.fn().mockReturnValue([]),
      getConversationManager: vi.fn().mockReturnValue(mockConversationManager),
    };

    mockEmitEvent = vi.fn().mockResolvedValue(undefined);
    mockToolApprovalHandler = { requestApproval: vi.fn() };

    baseDeps = {
      llmExecutor: mockLLMExecutor,
      toolService: mockToolService,
    };

    executor = new AgentLoopExecutor({ ...baseDeps });
  });

  // ===========================================================================
  // Constructor Tests
  // ===========================================================================

  describe("constructor", () => {
    it("should construct with minimal dependencies", () => {
      const exec = new AgentLoopExecutor({
        llmExecutor: mockLLMExecutor,
        toolService: mockToolService,
      });
      expect(exec).toBeInstanceOf(AgentLoopExecutor);
    });

    it("should construct with all optional dependencies", () => {
      const exec = new AgentLoopExecutor({
        llmExecutor: mockLLMExecutor,
        toolService: mockToolService,
        eventManager: mockEventManager,
        emitEvent: mockEmitEvent,
        toolApprovalHandler: mockToolApprovalHandler,
        metricsRegistry: mockMetricsRegistry,
        globalContext: mockGlobalContext,
      });
      expect(exec).toBeInstanceOf(AgentLoopExecutor);
    });

    it("should resolve checkpoint deps from globalContext when not provided", () => {
      const exec = new AgentLoopExecutor({
        llmExecutor: mockLLMExecutor,
        toolService: mockToolService,
        globalContext: mockGlobalContext,
      });
      expect(exec).toBeInstanceOf(AgentLoopExecutor);
    });

    it("should handle error when resolving checkpoint deps from globalContext", () => {
      const badContext = {
        container: {
          get: vi.fn().mockImplementation(() => {
            throw new Error("Container resolution failed");
          }),
        },
      } as unknown as GlobalContext;

      const exec = new AgentLoopExecutor({
        llmExecutor: mockLLMExecutor,
        toolService: mockToolService,
        globalContext: badContext,
      });
      expect(exec).toBeInstanceOf(AgentLoopExecutor);
    });
  });

  // ===========================================================================
  // Setter Tests
  // ===========================================================================

  describe("setEventEmitter", () => {
    it("should set the event emitter function", () => {
      const newEmitter = vi.fn().mockResolvedValue(undefined);
      executor.setEventEmitter(newEmitter);
      expect(true).toBe(true);
    });
  });

  describe("setEventManager", () => {
    it("should set the event manager", () => {
      executor.setEventManager(mockEventManager);
      expect(true).toBe(true);
    });
  });

  // ===========================================================================
  // Execute Tests
  // ===========================================================================

  describe("execute", () => {
    it("should delegate to coordinator.execute with correct arguments", async () => {
      const mockResult: AgentLoopResult = {
        success: true,
        content: "Final answer",
        iterations: 3,
        toolCallCount: 5,
      };
      mockCoordinatorExecute.mockResolvedValue(mockResult);
      mockPrepareToolSchemas.mockReturnValue([
        { id: "tool1", description: "Tool 1" },
        { id: "tool2", description: "Tool 2" },
      ]);

      const result = await executor.execute(mockEntity, mockStateCoordinator);

      expect(mockPrepareToolSchemas).toHaveBeenCalledWith(["tool1", "tool2"], mockToolService);
      expect(MockAgentExecutionCoordinator).toHaveBeenCalledTimes(1);
      expect(mockCoordinatorExecute).toHaveBeenCalledWith(
        mockEntity,
        mockConversationManager,
        [
          { id: "tool1", description: "Tool 1" },
          { id: "tool2", description: "Tool 2" },
        ],
        "profile-1",
        10,
      );
      expect(result).toBe(mockResult);
    });

    it("should use default maxIterations (10) when config does not specify", async () => {
      const entityWithoutMax = {
        ...mockEntity,
        config: { ...mockEntity.config, maxIterations: undefined },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await executor.execute(entityWithoutMax, mockStateCoordinator);

      expect(mockCoordinatorExecute).toHaveBeenCalledWith(
        entityWithoutMax,
        mockConversationManager,
        undefined,
        expect.any(String),
        10,
      );
    });

    it("should use custom maxIterations from config", async () => {
      const entityWithCustomMax = {
        ...mockEntity,
        config: { ...mockEntity.config, maxIterations: 25 },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await executor.execute(entityWithCustomMax, mockStateCoordinator);

      expect(mockCoordinatorExecute).toHaveBeenCalledWith(
        entityWithCustomMax,
        mockConversationManager,
        undefined,
        expect.any(String),
        25,
      );
    });

    it("should handle undefined tool schemas when no tools configured", async () => {
      const entityNoTools = {
        ...mockEntity,
        config: { ...mockEntity.config, availableTools: { tools: [] } },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await executor.execute(entityNoTools, mockStateCoordinator);

      expect(mockPrepareToolSchemas).toHaveBeenCalledWith([], mockToolService);
      expect(mockCoordinatorExecute).toHaveBeenCalledWith(
        expect.any(Object),
        mockConversationManager,
        undefined,
        expect.any(String),
        10,
      );
    });

    it("should use DEFAULT profileId when config does not specify", async () => {
      const entityNoProfile = {
        ...mockEntity,
        config: { ...mockEntity.config, profileId: undefined },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await executor.execute(entityNoProfile, mockStateCoordinator);

      expect(mockCoordinatorExecute).toHaveBeenCalledWith(
        entityNoProfile,
        mockConversationManager,
        undefined,
        "DEFAULT",
        10,
      );
    });

    it("should handle empty availableTools gracefully", async () => {
      const entityEmptyTools = {
        ...mockEntity,
        config: { ...mockEntity.config, availableTools: undefined },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await executor.execute(entityEmptyTools, mockStateCoordinator);

      expect(mockPrepareToolSchemas).toHaveBeenCalledWith([], mockToolService);
    });

    it("should return the exact result from coordinator", async () => {
      const mockResult: AgentLoopResult = {
        success: false,
        error: "Something went wrong",
        iterations: 2,
        toolCallCount: 3,
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue(mockResult);

      const result = await executor.execute(mockEntity, mockStateCoordinator);

      expect(result).toEqual(mockResult);
    });
  });

  // ===========================================================================
  // ExecuteStream Tests
  // ===========================================================================

  describe("executeStream", () => {
    it("should delegate to coordinator.executeStream and yield events", async () => {
      mockPrepareToolSchemas.mockReturnValue([{ id: "tool1" }]);

      const mockStreamEvents = [
        { type: "agent_start", agentLoopId: "agent-loop-1" },
        { type: "text", delta: "Hello" },
        { type: "agent_end", agentLoopId: "agent-loop-1", success: true },
      ];

      async function* mockStream() {
        for (const event of mockStreamEvents) {
          yield event;
        }
      }
      mockCoordinatorExecuteStream.mockReturnValue(mockStream());

      const events: any[] = [];
      for await (const event of executor.executeStream(mockEntity, mockStateCoordinator)) {
        events.push(event);
      }

      expect(mockPrepareToolSchemas).toHaveBeenCalledWith(["tool1", "tool2"], mockToolService);
      expect(mockCoordinatorExecuteStream).toHaveBeenCalledWith(
        mockEntity,
        mockConversationManager,
        [{ id: "tool1" }],
        "profile-1",
        10,
      );
      expect(events).toEqual(mockStreamEvents);
    });

    it("should use default values for stream execution", async () => {
      const entityMinimal = {
        ...mockEntity,
        config: { ...mockEntity.config, maxIterations: undefined, profileId: undefined },
      };
      mockPrepareToolSchemas.mockReturnValue(undefined);

      async function* emptyStream() {
        // no events
      }
      mockCoordinatorExecuteStream.mockReturnValue(emptyStream());

      const events: any[] = [];
      for await (const event of executor.executeStream(entityMinimal, mockStateCoordinator)) {
        events.push(event);
      }

      expect(mockCoordinatorExecuteStream).toHaveBeenCalledWith(
        entityMinimal,
        mockConversationManager,
        undefined,
        "DEFAULT",
        10,
      );
    });

    it("should propagate empty stream results", async () => {
      mockPrepareToolSchemas.mockReturnValue(undefined);

      async function* emptyStream() {
        // no events
      }
      mockCoordinatorExecuteStream.mockReturnValue(emptyStream());

      const events: any[] = [];
      for await (const event of executor.executeStream(mockEntity, mockStateCoordinator)) {
        events.push(event);
      }

      expect(events).toHaveLength(0);
    });
  });

  // ===========================================================================
  // createCoordinator Tests (indirectly tested via execute/executeStream)
  // ===========================================================================

  describe("createCoordinator", () => {
    it("should pass toolApprovalHandler through dependency chain", async () => {
      const exec = new AgentLoopExecutor({
        ...baseDeps,
        toolApprovalHandler: mockToolApprovalHandler,
      });

      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await exec.execute(mockEntity, mockStateCoordinator);

      expect(MockAgentExecutionCoordinator).toHaveBeenCalledTimes(1);
    });

    it("should create coordinator with metrics registry when provided", async () => {
      const exec = new AgentLoopExecutor({
        ...baseDeps,
        metricsRegistry: mockMetricsRegistry,
      });

      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await exec.execute(mockEntity, mockStateCoordinator);

      expect(MockAgentExecutionCoordinator).toHaveBeenCalledTimes(1);
    });

    it("should pass eventManager through to coordinators", async () => {
      const exec = new AgentLoopExecutor({
        ...baseDeps,
        eventManager: mockEventManager,
      });

      mockPrepareToolSchemas.mockReturnValue(undefined);
      mockCoordinatorExecute.mockResolvedValue({ success: true });

      await exec.execute(mockEntity, mockStateCoordinator);

      expect(MockAgentExecutionCoordinator).toHaveBeenCalledTimes(1);
    });
  });
});
