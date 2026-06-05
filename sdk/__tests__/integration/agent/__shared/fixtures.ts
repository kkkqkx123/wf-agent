/**
 * Agent Loop Integration Test Fixtures
 *
 * Provides factory functions for creating AgentLoopCoordinator instances
 * with mock dependencies for integration testing.
 */

import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { AgentLoopRegistry } from "@/agent/stores/agent-loop-registry.js";
import { AgentLoopCoordinator } from "@/agent/execution/coordinators/agent-loop-coordinator.js";
import { MemoryAgentLoopStorage } from "@wf-agent/storage";
import { MockLLMService } from "../../__shared/mock-llm-service.js";
import * as Identifiers from "@/core/di/service-identifiers.js";
import { InterruptionState } from "@/core/utils/interruption/interruption-state.js";
import type { ExecutionDomainContext } from "@wf-agent/types";

// =============================================================================
// Constants
// =============================================================================

export const MOCK_PROFILE_ID = "integration-test-mock-llm";
export const TEST_TIMEOUT = 10000;

// =============================================================================
// Types
// =============================================================================

export interface AgentLoopTestFixture {
  coordinator: AgentLoopCoordinator;
  registry: AgentLoopRegistry;
  mockLLM: MockLLMService;
  storage: MemoryAgentLoopStorage;
}

// =============================================================================
// Helpers
// =============================================================================

function createMockEventManager(): any {
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

function createMockGlobalContext(): any {
  const interruptionStateFactory = {
    create: (executionId: string, context?: ExecutionDomainContext) => new InterruptionState({ contextId: executionId, context }),
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
 * Create a basic Agent Loop Runtime Config for testing
 */
export function createBasicAgentConfig(overrides?: Partial<AgentLoopRuntimeConfig>): AgentLoopRuntimeConfig {
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
