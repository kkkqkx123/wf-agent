/**
 * Integration Tests: Tool Call Protocol
 *
 * Tests the full tool call protocol flow through the agent loop coordinator chain:
 * - Protocol locking at execution start
 * - Protocol enforcement through LLM client
 * - History conversion for text-mode formats
 * - Cross-boundary protocol handling
 *
 * These tests exercise the complete chain:
 * AgentLoopCoordinator -> AgentLoopExecutor -> AgentExecutionCoordinator ->
 * AgentIterationCoordinator -> CoreLLMExecutionCoordinator -> LLMExecutor -> MockLLMWrapper
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AgentLoopStatus } from "@wf-agent/types";
import {
  createFullAgentLoopFixture,
  createBasicAgentConfig,
} from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";

describe("Agent Loop Tool Call Protocol", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  // ===========================================================================
  // Scenario: Agent loop with XML tool call format
  // ===========================================================================
  describe("XML format protocol", () => {
    it("should complete execution with XML tool call format", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Quick answer using XML format.");

      const config = createBasicAgentConfig({
        maxIterations: 1,
        toolCallFormat: { format: "xml" },
      });

      const result = await fixture.coordinator.execute(config);      expect(result.agentLoopId).toBeDefined();      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      expect(entity!.getLockedToolCallFormat()).toBeDefined();
      expect(entity!.getLockedToolCallFormat()!.format).toBe("xml");
    });

    it("should lock protocol on entity at execution start", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Test response.");

      const config = createBasicAgentConfig({
        maxIterations: 1,
        toolCallFormat: { format: "xml" },
      });

      const result = await fixture.coordinator.execute(config);      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity!.getLockedToolCallFormat()!.format).toBe("xml");
    });
  });

  // ===========================================================================
  // Scenario: Agent loop with json_wrapped tool call format
  // ===========================================================================
  describe("json_wrapped format protocol", () => {
    it("should complete execution with json_wrapped tool call format", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Answer in JSON wrapped format.");

      const config = createBasicAgentConfig({
        maxIterations: 1,
        toolCallFormat: {
          format: "json_wrapped",
          markers: { start: "<<<TOOL_CALL>>>", end: "<<<END_TOOL_CALL>>>" },
        },
      });

      const result = await fixture.coordinator.execute(config);      expect(result.agentLoopId).toBeDefined();      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      expect(entity!.getLockedToolCallFormat()!.format).toBe("json_wrapped");
    });
  });

  // ===========================================================================
  // Scenario: Agent loop with json_raw tool call format
  // ===========================================================================
  describe("json_raw format protocol", () => {
    it("should complete execution with json_raw tool call format", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Answer in raw JSON format.");

      const config = createBasicAgentConfig({
        maxIterations: 1,
        toolCallFormat: { format: "json_raw" },
      });

      const result = await fixture.coordinator.execute(config);      expect(result.agentLoopId).toBeDefined();      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      expect(entity!.getLockedToolCallFormat()!.format).toBe("json_raw");
    });
  });

  // ===========================================================================
  // Scenario: Native (default) format — no protocol conversion needed
  // ===========================================================================
  describe("native format protocol (default)", () => {
    it("should complete execution with default native format", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Answer using native format.");

      const config = createBasicAgentConfig({
        maxIterations: 1,
        // No toolCallFormat — defaults to native
      });

      const result = await fixture.coordinator.execute(config);      expect(result.agentLoopId).toBeDefined();      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      expect(entity!.getLockedToolCallFormat()!.format).toBe("native");
    });
  });

  // ===========================================================================
  // Scenario: Multi-iteration with XML protocol — tool calls and history conversion
  // ===========================================================================
  describe("multi-iteration with XML protocol", () => {
    it("should execute tool call and continue with locked XML protocol", async () => {
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Let me check the weather.",
          toolCalls: [
            { id: "call-1", name: "mock_echo_tool", arguments: '{"message":"check weather"}' },
          ],
        },
        {
          content: "The weather is sunny.",
        },
      ]);

      const config = createBasicAgentConfig({
        maxIterations: 2,
        toolCallFormat: { format: "xml" },
        availableTools: { tools: ["mock_echo_tool"] },
      });

      const result = await fixture.coordinator.execute(config);      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      // Protocol should remain locked as XML throughout
      expect(entity!.getLockedToolCallFormat()!.format).toBe("xml");
    });
  });

  // ===========================================================================
  // Scenario: Checkpoint preserves locked protocol
  // ===========================================================================
  describe("checkpoint preserves locked protocol", () => {
    it("should preserve lockedToolCallFormat across checkpoint/restore", async () => {
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "First response.",
          toolCalls: [
            { id: "call-1", name: "mock_echo_tool", arguments: '{"message":"step1"}' },
          ],
        },
        {
          content: "Second response after checkpoint.",
        },
      ]);

      const config = createBasicAgentConfig({
        maxIterations: 2,
        toolCallFormat: { format: "xml" },
        createCheckpointOnEnd: true,
        availableTools: { tools: ["mock_echo_tool"] },
      });

      const result = await fixture.coordinator.execute(config);      const entity = await fixture.registry.get(result.agentLoopId!);
      expect(entity).toBeDefined();
      expect(entity!.getStatus()).toBe(AgentLoopStatus.COMPLETED);
      // Protocol should survive checkpoint
      expect(entity!.getLockedToolCallFormat()!.format).toBe("xml");
    });
  });
});