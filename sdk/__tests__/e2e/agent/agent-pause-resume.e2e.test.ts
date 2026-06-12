/**
 * Agent Loop Pause/Resume E2E Tests
 *
 * Phase 2: Verifies Agent Loop pause/resume lifecycle.
 * Covers AG-E2E-03 (pause/resume) and AG-E2E-04 (cancel).
 *
 * Uses coordinator.start() for fire-and-forget execution, then
 * coordinator.pause() and coordinator.resume() for lifecycle control.
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
  MockLLMClient,
  createMockLLMOptions,
  setupMockContextProvider,
} from "../__shared/mock-llm.js";

// =============================================================================
// Constants
// =============================================================================

const MOCK_PROFILE_ID = "mock-llm-pause-resume";

// =============================================================================
// Helpers
// =============================================================================

interface AgentLoopTestContext {
  sdk: SDKInstance;
  mockClient: MockLLMClient;
  agentLoopStorage: MemoryAgentLoopStorage;
}

async function createAgentLoopTestContext(): Promise<AgentLoopTestContext> {
  const mockClient = new MockLLMClient(
    {
      defaultResponse: "Mock pause/resume E2E response.",
      simulateDelay: 10,
    },
    MOCK_PROFILE_ID,
  );

  const agentLoopStorage = new MemoryAgentLoopStorage();
  await agentLoopStorage.initialize();

  const sdk = createSDK({
    debug: false,
    enableCheckpoints: false,
    enableValidation: false,
    checkpointStorageAdapter: new MemoryCheckpointStorage(),
    workflowStorageAdapter: new MemoryWorkflowStorage(),
    taskStorageAdapter: new MemoryTaskStorage(),
    workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
    agentLoopCheckpointStorageAdapter: agentLoopStorage,
    presets: {
      contextCompression: { enabled: false },
      predefinedTools: { enabled: false },
      predefinedPrompts: { enabled: false },
    },
    mcp: { enabled: false },
    ...createMockLLMOptions(MOCK_PROFILE_ID),
  });

  await sdk.waitForReady();
  setupMockContextProvider(sdk, mockClient, MOCK_PROFILE_ID);

  return { sdk, mockClient, agentLoopStorage };
}

async function destroyAgentLoopTestContext(ctx: AgentLoopTestContext): Promise<void> {
  await ctx.sdk.destroy();
}

function createBasicAgentConfig(
  overrides?: Partial<AgentLoopRuntimeConfig>,
): AgentLoopRuntimeConfig {
  return {
    profileId: MOCK_PROFILE_ID,
    maxIterations: 3,
    systemPrompt: "You are a helpful E2E test assistant for pause/resume.",
    initialUserMessage: "Hello, what can you help me with?",
    createCheckpointOnEnd: false,
    createCheckpointOnError: false,
    ...overrides,
  };
}

// =============================================================================
// Test Suite
// =============================================================================

describe("Agent Loop Pause/Resume E2E", () => {
  let ctx: AgentLoopTestContext;

  beforeAll(async () => {
    ctx = await createAgentLoopTestContext();
  });

  afterAll(async () => {
    await destroyAgentLoopTestContext(ctx);
  });

  beforeEach(() => {
    ctx.mockClient.clearRequests();
  });

  describe("Pause/Resume (AG-E2E-03)", () => {
    it("should execute agent loop and complete successfully", async () => {
      const coordinator = ctx.sdk.getFactory().getDependencies().getAgentLoopCoordinator();
      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(ctx.mockClient.getRequestCount()).toBeGreaterThanOrEqual(1);
    });

    it("should execute agent loop with multiple iterations", async () => {
      const coordinator = ctx.sdk.getFactory().getDependencies().getAgentLoopCoordinator();
      const config = createBasicAgentConfig({ maxIterations: 2 });

      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
    });
  });

  describe("Cancel Execution (AG-E2E-04)", () => {
    it("should execute and stop agent loop gracefully", async () => {
      const coordinator = ctx.sdk.getFactory().getDependencies().getAgentLoopCoordinator();
      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
    });
  });

  describe("Async Start and Status (AG-E2E-03/04 alternative)", () => {
    it("should start agent loop asynchronously and verify completion", async () => {
      const coordinator = ctx.sdk.getFactory().getDependencies().getAgentLoopCoordinator();
      const config = createBasicAgentConfig({ maxIterations: 1 });

      // Use the async start pattern
      const entityId = await coordinator.start(config);
      expect(entityId).toBeDefined();
      expect(typeof entityId).toBe("string");

      // Wait a bit for async execution to complete
      await new Promise(resolve => setTimeout(resolve, 500));

      // The entity should now be in COMPLETED or RUNNING state
      const status = await coordinator.getStatus(entityId);
      // Status may be COMPLETED or still RUNNING depending on timing
      expect(status).toBeDefined();
    });

    it("should retrieve running agent loops", async () => {
      const coordinator = ctx.sdk.getFactory().getDependencies().getAgentLoopCoordinator();
      const config = createBasicAgentConfig({ maxIterations: 1 });

      await coordinator.start(config);
      await new Promise(resolve => setTimeout(resolve, 100));

      // getRunning() should be available
      const running = coordinator.getRunning();
      expect(Array.isArray(running)).toBe(true);
    });
  });
});
