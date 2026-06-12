/**
 * Agent Loop Integration Test Fixtures
 *
 * Provides factory functions for creating AgentLoopCoordinator instances
 * with mock dependencies for integration testing.
 *
 * Two fixture levels:
 * 1. createAgentLoopFixture() - Minimal executor (for coordinator-level tests)
 * 2. createFullAgentLoopFixture() - Full coordinator chain with mock LLM (for integration tests)
 */

import type { AgentLoopRuntimeConfig, Tool, AgentHookTriggeredEvent } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { AgentLoopRegistry } from "@/agent/stores/agent-loop-registry.js";
import { AgentLoopCoordinator } from "@/agent/execution/coordinators/agent-loop-coordinator.js";
import { MemoryAgentLoopStorage } from "@wf-agent/storage";
import { MockLLMService } from "../../__shared/mock-llm-service.js";
import * as Identifiers from "@/core/di/service-identifiers.js";
import { InterruptionState } from "@/core/utils/interruption/interruption-state.js";
import type { ExecutionDomainContext } from "@wf-agent/types";

// Full-chain dependencies
import { MockLLMWrapper } from "./mock-llm-wrapper.js";
import { LLMExecutor } from "@/core/executors/llm-executor.js";
import { ToolRegistry } from "@/core/registry/tool-registry.js";
import { AgentLoopExecutor } from "@/agent/execution/executors/agent-loop-executor.js";
import type { EventRegistry } from "@/core/registry/event-registry.js";

// =============================================================================
// Constants
// =============================================================================

export const MOCK_PROFILE_ID = "integration-test-mock-llm";
export const MOCK_ECHO_TOOL_ID = "mock_echo_tool";
export const TEST_TIMEOUT = 10000;

// =============================================================================
// Mock Tool Definitions
// =============================================================================

/**
 * A simple echo tool for testing tool execution in agent loop integration tests.
 * Returns the input message as part of the output for easy verification.
 */
export const mockEchoTool: Tool = {
  id: MOCK_ECHO_TOOL_ID,
  description: "A mock tool that echoes back the input for testing.",
  type: "STATELESS",
  parameters: {
    type: "object",
    properties: {
      message: {
        type: "string",
        description: "The message to echo back",
      },
    },
    required: ["message"],
  },
  config: {
    execute: async (_params: Record<string, unknown>) => {
      return {
        success: true,
        data: { echo: (_params["message"] as string) || "no message" },
      };
    },
  },
};

// =============================================================================
// Types
// =============================================================================

export interface AgentLoopTestFixture {
  coordinator: AgentLoopCoordinator;
  registry: AgentLoopRegistry;
  mockLLM: MockLLMService;
  storage: MemoryAgentLoopStorage;
}

/**
 * Full chain integration test fixture.
 * Uses a fully wired AgentLoopExecutor with MockLLMWrapper,
 * so the entire coordinator chain is exercised.
 */
export interface FullAgentLoopTestFixture {
  coordinator: AgentLoopCoordinator;
  registry: AgentLoopRegistry;
  mockLLMWrapper: MockLLMWrapper;
  toolRegistry: ToolRegistry;
  storage: MemoryAgentLoopStorage;
}

// =============================================================================
// Helpers
// =============================================================================

export function createMockEventManager(): any {
  return {
    on: () => {},
    off: () => {},
    removeListener: () => {},
    emit: async () => {},
    cleanupExecutionListeners: () => 0,
    getListeners: () => [],
    clear: () => {},
    listenerCount: () => 0,
  };
}

export function createMockGlobalContext(): any {
  const interruptionStateFactory = {
    create: (executionId: string, context?: ExecutionDomainContext) =>
      new InterruptionState({ contextId: executionId, context }),
  };
  return {
    container: {
      get: (id: symbol) => {
        if (id === Identifiers.InterruptionState) {
          return interruptionStateFactory;
        }
        return undefined;
      },
    },
    eventRegistry: createMockEventManager(),
    config: {},
  };
}

/**
 * Create a minimal AgentLoopCoordinator for integration testing
 * Uses MockLLMService to simulate LLM responses
 */
export async function createAgentLoopFixture(): Promise<AgentLoopTestFixture> {
  const storage = new MemoryAgentLoopStorage();
  await storage.initialize();

  const registry = new AgentLoopRegistry({ storageAdapter: storage });
  const mockLLM = new MockLLMService();
  const eventManager = createMockEventManager();
  const globalContext = createMockGlobalContext();

  const coordinator = new AgentLoopCoordinator(
    registry,
    mockLLM.createExecutor(),
    globalContext,
    eventManager,
    undefined, // metrics collector
  );

  return { coordinator, registry, mockLLM, storage };
}

/**
 * Create an event registry mock with the required EventRegistry interface.
 */
function createEventRegistry(): EventRegistry {
  return {
    on: () => {},
    off: () => {},
    removeListener: () => {},
    emit: async () => {},
    cleanupExecutionListeners: () => 0,
    getListeners: () => [],
    clear: () => {},
    listenerCount: () => 0,
  } as unknown as EventRegistry;
}

/**
 * Create a full-chain AgentLoopCoordinator for integration testing.
 *
 * Wires up the complete coordinator chain:
 * AgentLoopCoordinator -> AgentLoopExecutor -> AgentExecutionCoordinator ->
 * AgentIterationCoordinator -> CoreLLMExecutionCoordinator -> LLMExecutor -> MockLLMWrapper
 *
 * ToolRegistry is also fully wired, allowing tool execution tests.
 *
 * @param registerTools Whether to register the mock echo tool (default: true)
 * @returns FullAgentLoopTestFixture
 */
export async function createFullAgentLoopFixture(
  registerTools: boolean = true,
): Promise<FullAgentLoopTestFixture> {
  const storage = new MemoryAgentLoopStorage();
  await storage.initialize();

  const registry = new AgentLoopRegistry({ storageAdapter: storage });
  const eventManager = createEventRegistry();
  const mockLLMWrapper = new MockLLMWrapper();

  // Real LLM Executor with Mock Wrapper
  const llmExecutor = new LLMExecutor(mockLLMWrapper);

  // Real Tool Registry (no persistence for tests)
  const toolRegistry = new ToolRegistry({}, null);

  // Register mock tool for tool execution tests
  if (registerTools) {
    toolRegistry.register(mockEchoTool, { skipIfExists: true });
  }

  // Emit agent hook event function
  const emitAgentEvent = async (_event: AgentHookTriggeredEvent): Promise<void> => {
    // no-op for tests that don't need hook verification
  };

  // Real AgentLoopExecutor (creates its own coordinators internally)
  const executor = new AgentLoopExecutor({
    llmExecutor,
    toolService: toolRegistry,
    eventManager,
    emitEvent: emitAgentEvent,
  });

  const globalContext = createMockGlobalContext();

  const coordinator = new AgentLoopCoordinator(
    registry,
    executor,
    globalContext,
    eventManager,
    undefined, // metrics collector
  );

  return { coordinator, registry, mockLLMWrapper, toolRegistry, storage };
}

/**
 * Create a basic Agent Loop Runtime Config for testing
 */
export function createBasicAgentConfig(
  overrides?: Partial<AgentLoopRuntimeConfig>,
): AgentLoopRuntimeConfig {
  return {
    profileId: MOCK_PROFILE_ID,
    maxIterations: 1,
    systemPrompt: "You are a helpful integration test assistant.",
    initialUserMessage: "Hello, what can you help me with?",
    createCheckpointOnEnd: false,
    createCheckpointOnError: false,
    ...overrides,
  };
}

/**
 * Wait for a condition to become true, with timeout
 */
export async function waitForCondition(
  condition: () => boolean | Promise<boolean>,
  timeoutMs: number = 5000,
  intervalMs: number = 100,
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await condition()) {
      return;
    }
    await new Promise(resolve => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Condition not met within ${timeoutMs}ms`);
}

/**
 * Wait for agent loop to reach a specific status
 */
export async function waitForAgentStatus(
  registry: AgentLoopRegistry,
  id: string,
  expectedStatus: AgentLoopStatus,
  timeoutMs: number = 5000,
): Promise<void> {
  await waitForCondition(async () => {
    const entity = await registry.get(id);
    return entity?.getStatus() === expectedStatus;
  }, timeoutMs);
}
