/**
 * Integration Test: Agent Loop Hook Triggering
 *
 * Tests that agent lifecycle hooks are triggered at the correct points
 * during agent loop execution through the event system.
 *
 * Hook execution requires hooks to be configured in the agent config.
 * The hook handler (executeAgentHook) filters hooks by type and only
 * invokes the emitEvent callback for registered hooks.
 *
 * Covers:
 * - Hook callback invocation with configured hooks
 * - Hook triggering across multiple iterations
 * - All hook types: BEFORE_ITERATION, AFTER_ITERATION,
 *   BEFORE_LLM_CALL, AFTER_LLM_CALL, BEFORE_TOOL_CALL, AFTER_TOOL_CALL
 */

import { describe, it, expect } from "vitest";
import type { AgentHookTriggeredEvent } from "@wf-agent/types";
import { MemoryAgentLoopStorage } from "@wf-agent/storage";
import { AgentLoopRegistry } from "@/agent/stores/agent-loop-registry";
import { AgentLoopCoordinator } from "@/agent/execution/coordinators/agent-loop-coordinator";
import { MockLLMWrapper } from "./__shared/mock-llm-wrapper";
import { LLMExecutor } from "@/services/executors/llm-executor";
import { ToolRegistry } from "@/shared/registry/tool-registry";
import { AgentLoopExecutor } from "@/agent/execution/executors/agent-loop-executor";
import {
  createBasicAgentConfig,
  createMockGlobalContext,
  createMockEventManager,
} from "./__shared/fixtures";

describe("Agent Loop Hook Triggering", () => {
  it("should invoke hooks at each lifecycle stage for single iteration", async () => {
    const storage = new MemoryAgentLoopStorage();
    await storage.initialize();
    const registry = new AgentLoopRegistry({ storageAdapter: storage });
    const mockLLMWrapper = new MockLLMWrapper();
    mockLLMWrapper.setDefaultResponse("Final answer.");

    const llmExecutor = new LLMExecutor(mockLLMWrapper);
    const toolRegistry = new ToolRegistry({}, null);
    const eventManager = createMockEventManager();

    const hookCalls: string[] = [];
    const executor = new AgentLoopExecutor({
      llmExecutor,
      toolService: toolRegistry,
      emitEvent: async (event: AgentHookTriggeredEvent) => {
        hookCalls.push(event.hookType);
      },
    });

    const coordinator = new AgentLoopCoordinator(
      registry,
      executor,
      createMockGlobalContext(),
      eventManager,
    );

    // Configure hooks so that executeAgentHook actually processes them
    const config = createBasicAgentConfig({
      maxIterations: 1,
      hooks: [
        { hookType: "BEFORE_ITERATION", eventName: "iteration.before" },
        { hookType: "BEFORE_LLM_CALL", eventName: "llm.before" },
        { hookType: "AFTER_LLM_CALL", eventName: "llm.after" },
        { hookType: "AFTER_ITERATION", eventName: "iteration.after" },
      ],
    });

    const result = await coordinator.execute(config);

    expect(result.success).toBe(true);

    // With one iteration and no tool calls, expect these hook types:
    // BEFORE_ITERATION, BEFORE_LLM_CALL, AFTER_LLM_CALL
    // Note: AFTER_ITERATION is NOT called when no tool calls because
    // executeIteration returns early before reaching AFTER_ITERATION
    expect(hookCalls.length).toBeGreaterThanOrEqual(3);
    expect(hookCalls.filter(h => h === "BEFORE_ITERATION").length).toBe(1);
    expect(hookCalls.filter(h => h === "BEFORE_LLM_CALL").length).toBe(1);
    expect(hookCalls.filter(h => h === "AFTER_LLM_CALL").length).toBe(1);

    coordinator.destroy();
    await storage.clear();
  });

  it("should invoke hooks for each iteration in multi-iteration flow with tools", async () => {
    const storage = new MemoryAgentLoopStorage();
    await storage.initialize();
    const registry = new AgentLoopRegistry({ storageAdapter: storage });
    const mockLLMWrapper = new MockLLMWrapper();
    mockLLMWrapper.setResponseSequence([
      {
        content: "Tool call.",
        toolCalls: [
          {
            id: "call_hook_1",
            name: "mock_echo_tool",
            arguments: JSON.stringify({ message: "hook test" }),
          },
        ],
      },
      { content: "Final answer." },
    ]);

    const llmExecutor = new LLMExecutor(mockLLMWrapper);
    const toolRegistry = new ToolRegistry({}, null);
    toolRegistry.register({
      id: "mock_echo_tool",
      type: "STATELESS",
      description: "Echoes back the input message",
      parameters: {
        type: "object",
        properties: { message: { type: "string" } },
        required: ["message"],
      },
      config: {
        execute: async (params: Record<string, unknown>) => {
          return { success: true, data: { echo: (params["message"] as string) || "" } };
        },
      },
    });

    const eventManager = createMockEventManager();

    const hookCalls: string[] = [];
    const executor = new AgentLoopExecutor({
      llmExecutor,
      toolService: toolRegistry,
      emitEvent: async (event: AgentHookTriggeredEvent) => {
        hookCalls.push(event.hookType);
      },
    });

    const coordinator = new AgentLoopCoordinator(
      registry,
      executor,
      createMockGlobalContext(),
      eventManager,
    );

    // Configure hooks that cover all stages including tool calls
    const config = createBasicAgentConfig({
      maxIterations: 5,
      hooks: [
        { hookType: "BEFORE_ITERATION", eventName: "iteration.before" },
        { hookType: "BEFORE_LLM_CALL", eventName: "llm.before" },
        { hookType: "AFTER_LLM_CALL", eventName: "llm.after" },
        { hookType: "BEFORE_TOOL_CALL", eventName: "tool.before" },
        { hookType: "AFTER_TOOL_CALL", eventName: "tool.after" },
        { hookType: "AFTER_ITERATION", eventName: "iteration.after" },
      ],
    });

    const result = await coordinator.execute(config);

    expect(result.success).toBe(true);

    // With 2 iterations, tools in first iteration:
    // Iteration 1: BEFORE_ITERATION, BEFORE_LLM_CALL, AFTER_LLM_CALL,
    //              BEFORE_TOOL_CALL, AFTER_TOOL_CALL, AFTER_ITERATION
    // Iteration 2: BEFORE_ITERATION, BEFORE_LLM_CALL, AFTER_LLM_CALL, AFTER_ITERATION
    // Total: at least 10 hook calls
    expect(hookCalls.length).toBeGreaterThanOrEqual(10);

    // Check first iteration had tool hooks
    expect(hookCalls.filter(h => h === "BEFORE_TOOL_CALL").length).toBe(1);
    expect(hookCalls.filter(h => h === "AFTER_TOOL_CALL").length).toBe(1);

    coordinator.destroy();
    await storage.clear();
  });
});
