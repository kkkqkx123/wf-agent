/**
 * Agent Loop Execution E2E Tests
 *
 * Phase 2: Verifies Agent Loop execution lifecycle and basic behavior.
 * Covers AG-E2E-01 (basic execution).
 *
 * Uses MockHumanRelayHandler to simulate LLM responses without real API calls.
 *
 * NOTE: The AgentLoopCoordinator.execute() has a state transitor bug where
 * completeAgentLoop throws, leading to catch block attempting failAgentLoop
 * on an already COMPLETED entity. The execution itself works correctly
 * (iterations > 0, content returned), but result.success may be false.
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createSDK } from "@/api/index.js";
import type { SDKInstance } from "@/api/index.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryWorkflowExecutionStorage,
  MemoryTaskStorage,
  MemoryAgentLoopStorage,
} from "@wf-agent/storage";
import {
  MockHumanRelayHandler,
  createMockLLMOptions,
  setupMockContextProvider,
} from "../__shared/mock-llm.js";

// =============================================================================
// Constants
// =============================================================================

const MOCK_PROFILE_ID = "mock-llm";

// =============================================================================
// Helpers
// =============================================================================

interface AgentLoopTestContext {
  sdk: SDKInstance;
  mockHandler: MockHumanRelayHandler;
}

async function createAgentLoopTestContext(): Promise<AgentLoopTestContext> {
  const mockHandler = new MockHumanRelayHandler({
    defaultResponse: "This is a mock LLM response for E2E testing.",
    simulateDelay: 5,
  });

  const sdk = createSDK({
    debug: false,
    enableCheckpoints: false,
    enableValidation: false,
    checkpointStorageAdapter: new MemoryCheckpointStorage(),
    workflowStorageAdapter: new MemoryWorkflowStorage(),
    taskStorageAdapter: new MemoryTaskStorage(),
    workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
    agentLoopCheckpointStorageAdapter: new MemoryAgentLoopStorage(),
    presets: {
      contextCompression: { enabled: false },
      predefinedTools: { enabled: false },
      predefinedPrompts: { enabled: false },
    },
    mcp: { enabled: false },
    ...createMockLLMOptions(mockHandler, MOCK_PROFILE_ID),
  });

  await sdk.waitForReady();
  setupMockContextProvider(sdk, MOCK_PROFILE_ID);

  return { sdk, mockHandler };
}

async function destroyAgentLoopTestContext(ctx: AgentLoopTestContext): Promise<void> {
  await ctx.sdk.destroy();
}

function getCoordinator(sdk: SDKInstance) {
  return sdk.getFactory().getDependencies().getAgentLoopCoordinator();
}

function createBasicAgentConfig(overrides?: Partial<AgentLoopRuntimeConfig>): AgentLoopRuntimeConfig {
  return {
    profileId: MOCK_PROFILE_ID,
    maxIterations: 1,
    systemPrompt: "You are a helpful E2E test assistant.",
    initialUserMessage: "Hello, what can you help me with?",
    createCheckpointOnEnd: false,
    createCheckpointOnError: false,
    ...overrides,
  };
}

// =============================================================================
// Test Suite
// =============================================================================

describe("Agent Loop Execution E2E", () => {
  let ctx: AgentLoopTestContext;

  beforeAll(async () => {
    ctx = await createAgentLoopTestContext();
  });

  afterAll(async () => {
    await destroyAgentLoopTestContext(ctx);
  });

  beforeEach(() => {
    ctx.mockHandler.clearRequests();
  });

  describe("Basic Execution (AG-E2E-01)", () => {
    it("should execute a basic agent loop and return a result", async () => {
      const coordinator = getCoordinator(ctx.sdk);
      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await coordinator.execute(config);

      // The execution engine runs correctly:
      // - LLM profile "mock-llm" is found (HUMAN_RELAY provider)
      // - The mock handler is called with the user message
      // - The handler returns "Mock LLM response for E2E testing"
      // 
      // State transitor bug is now fixed - the coordinator checks entity status
      // before attempting state transition, so result.success should be true.
      expect(result.success).toBe(true);
      expect(ctx.mockHandler.getRequestCount()).toBeGreaterThanOrEqual(1);
    });
  });

  describe("Multiple Iterations (AG-E2E-01)", () => {
    it.skip("should execute multiple iterations when configured", () => {
      // TODO: Enable when agent loop executor properly handles multi-iteration flow
    });

    it("should execute a single iteration when maxIterations is 1", async () => {
      const coordinator = getCoordinator(ctx.sdk);
      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await coordinator.execute(config);

      // State transitor bug is now fixed - completeAgentLoop no longer
      // throws when entity is already COMPLETED by the executor.
      expect(result.success).toBe(true);
    });
  });

  describe("Agent Loop Lifecycle (AG-E2E-02)", () => {
    it.skip("should transition through CREATED -> RUNNING -> COMPLETED statuses", () => {
      // TODO: Enable when registry.get() entity type issues are resolved
    });
  });

  describe("Tool Call Recording (AG-E2E-06)", () => {
    it.skip("should execute agent loop with available tools", () => {
      // TODO: Enable when tool execution integration is tested
    });
  });

  describe("Error Handling (AG-E2E-08)", () => {
    it.skip("should handle error during agent loop execution gracefully", () => {
      // TODO: Enable when error handling in agent loop is verified
    });
  });
});
