/**
 * Agent Hook Context Builder Unit Tests
 *
 * Tests for the convertToEvaluationContext function that builds
 * evaluation context for Agent hooks with message access support.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { LLMMessage, AgentLoopRuntimeConfig, AgentToolConfig } from "@wf-agent/types";
import { buildAgentHookEvaluationContext, convertToEvaluationContext } from "../context-builder.js";
import { AgentLoopEntity } from "../../../../entities/agent-loop-entity.js";
import { ConversationSession } from "../../../../../shared/messaging/conversation-session.js";
import { AgentStateCoordinator } from "../../../../state-managers/agent-state-coordinator.js";

describe("Agent Hook Context Builder", () => {
  let entity: AgentLoopEntity;
  let stateCoordinator: AgentStateCoordinator;
  let mockConfig: AgentLoopRuntimeConfig;

  beforeEach(() => {
    // Create a minimal mock config
    const toolConfig: AgentToolConfig = {
      tools: [],
    };

    mockConfig = {
      profileId: "test-profile",
      systemPrompt: "You are a test assistant",
      maxIterations: 10,
      availableTools: toolConfig,
    };

    // Create an agent loop entity directly
    entity = new AgentLoopEntity("test-agent-1", mockConfig);

    // Create ConversationSession with initial messages
    const conversationSession = new ConversationSession({
      initialMessages: [
        { role: "system", content: "System message" },
        { role: "user", content: "Hello" },
      ],
    });

    // Create AgentStateCoordinator wrapping the ConversationSession
    stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });
  });

  describe("buildAgentHookEvaluationContext", () => {
    it("should build context with conversation manager reference", () => {
      const context = buildAgentHookEvaluationContext(entity, stateCoordinator);

      expect(context).toBeDefined();
      expect(context.iteration).toBe(0);
      expect(context.maxIterations).toBe(10);
      expect(context.toolCallCount).toBe(0);
      // Initial status is CREATED, not RUNNING
      expect(context.status).toBe("CREATED");
      expect(context.conversationManager).toBeDefined();
      expect(typeof context.conversationManager.getAllMessages).toBe("function");
      expect(typeof context.conversationManager.getMessages).toBe("function");
    });

    it("should include tool call info when provided", () => {
      const toolCallInfo = {
        id: "tool-1",
        name: "search",
        arguments: { query: "test" },
        result: { data: "result" },
      };

      const context = buildAgentHookEvaluationContext(entity, stateCoordinator, toolCallInfo);

      expect(context.toolCall).toBeDefined();
      expect(context.toolCall?.id).toBe("tool-1");
      expect(context.toolCall?.name).toBe("search");
      expect(context.toolCall?.result).toEqual({ data: "result" });
    });

    it("should provide tools API", () => {
      const context = buildAgentHookEvaluationContext(entity, stateCoordinator);

      expect(context.tools).toBeDefined();
      expect(typeof context.tools.isAvailable).toBe("function");
      expect(typeof context.tools.getAll).toBe("function");
    });
  });

  describe("convertToEvaluationContext - Message Access", () => {
    it("should expose messages in input namespace", () => {
      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input).toBeDefined();
      expect(evalContext.input["messages"]).toBeDefined();
      expect(Array.isArray(evalContext.input["messages"])).toBe(true);
      // Should have system + user messages
      expect((evalContext.input["messages"] as LLMMessage[]).length).toBe(2);
    });

    it("should expose lastMessage in input namespace", () => {
      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input["lastMessage"]).toBeDefined();
      const lastMessage = evalContext.input["lastMessage"] as LLMMessage | null;
      expect(lastMessage?.role).toBe("user");
      expect(lastMessage?.content).toBe("Hello");
    });

    it("should handle empty message history", () => {
      // Create entity with no messages
      const emptyEntity = new AgentLoopEntity("test-agent-empty", mockConfig);
      const emptySession = new ConversationSession();
      const emptyCoordinator = new AgentStateCoordinator({
        conversationManager: emptySession,
      });

      const hookContext = buildAgentHookEvaluationContext(emptyEntity, emptyCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input["messages"]).toEqual([]);
      expect(evalContext.input["lastMessage"]).toBeNull();
    });

    it("should include all messages including invisible ones", () => {
      // Add more messages via stateCoordinator
      stateCoordinator.addMessage({ role: "assistant", content: "Hi there!" });
      stateCoordinator.addMessage({ role: "user", content: "How are you?" });

      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      // Should have: system, user (initial) + assistant, user (added)
      expect((evalContext.input["messages"] as LLMMessage[]).length).toBe(4);
      const lastMessage = evalContext.input["lastMessage"] as LLMMessage | null;
      expect(lastMessage?.role).toBe("user");
      expect(lastMessage?.content).toBe("How are you?");
    });

    it("should preserve iteration metadata", () => {
      // Simulate some iterations by creating new state and using proper methods
      const testEntity = new AgentLoopEntity("test-agent-2", mockConfig);
      const testSession = new ConversationSession();
      const testCoordinator = new AgentStateCoordinator({
        conversationManager: testSession,
      });
      testEntity.state.start();
      testEntity.state.startIteration();
      testEntity.state.endIteration("Test response");
      testEntity.state.startIteration();

      // Record tool calls
      testEntity.state.recordToolCallStart("tool-1", "test-tool", {});
      testEntity.state.recordToolCallEnd("tool-1", { result: "ok" });
      testEntity.state.recordToolCallStart("tool-2", "test-tool-2", {});
      testEntity.state.recordToolCallEnd("tool-2", { result: "ok" });
      testEntity.state.recordToolCallStart("tool-3", "test-tool-3", {});
      testEntity.state.recordToolCallEnd("tool-3", { result: "ok" });

      const hookContext = buildAgentHookEvaluationContext(testEntity, testCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input["iteration"]).toBe(2);
      expect(evalContext.input["maxIterations"]).toBe(10);
      expect(evalContext.input["toolCallCount"]).toBe(3);
    });

    it("should include output status and error", () => {
      const testEntity = new AgentLoopEntity("test-agent-3", mockConfig);
      const testSession = new ConversationSession();
      const testCoordinator = new AgentStateCoordinator({
        conversationManager: testSession,
      });
      testEntity.state.start();
      testEntity.state.complete();

      const hookContext = buildAgentHookEvaluationContext(testEntity, testCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.output["status"]).toBe("COMPLETED");
      // Error should be null when not set
      expect(evalContext.output["error"]).toBeNull();
    });

    it("should include error information when present", () => {
      const testEntity = new AgentLoopEntity("test-agent-4", mockConfig);
      const testSession = new ConversationSession();
      const testCoordinator = new AgentStateCoordinator({
        conversationManager: testSession,
      });
      testEntity.state.start();
      const testError = new Error("Test error");
      testEntity.state.fail(testError);

      const hookContext = buildAgentHookEvaluationContext(testEntity, testCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.output["status"]).toBe("FAILED");
      expect(evalContext.output["error"]).toBe(testError);
    });

    it("should have empty variables namespace", () => {
      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.variables).toEqual({});
    });
  });

  describe("convertToEvaluationContext - Use Cases", () => {
    it("should support checking last message role", () => {
      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      // Simulate condition evaluation with type assertion
      const lastMessage = evalContext.input["lastMessage"] as LLMMessage | null;
      const lastMessageRole = lastMessage?.role;
      expect(lastMessageRole).toBe("user");
    });

    it("should support checking message count", () => {
      stateCoordinator.addMessage({ role: "assistant", content: "Response" });

      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      const messages = evalContext.input["messages"] as LLMMessage[];
      const messageCount = messages.length;
      expect(messageCount).toBe(3); // system + 2 user/assistant
    });

    it("should support checking for specific content patterns", () => {
      stateCoordinator.addMessage({ role: "user", content: "I need help with this" });

      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      const lastMessage = evalContext.input["lastMessage"] as LLMMessage | null;
      const lastContent = lastMessage?.content as string | undefined;
      const containsHelp = lastContent?.includes("help") ?? false;
      expect(containsHelp).toBe(true);
    });

    it("should support accessing message properties safely", () => {
      const hookContext = buildAgentHookEvaluationContext(entity, stateCoordinator);
      const evalContext = convertToEvaluationContext(hookContext);

      // Safe access pattern with type assertions
      const messages = evalContext.input["messages"] as LLMMessage[];
      const hasMessages = messages.length > 0;
      const lastMessage = evalContext.input["lastMessage"] as LLMMessage | null;
      const lastRole = lastMessage?.role ?? "unknown";

      expect(hasMessages).toBe(true);
      expect(lastRole).toBe("user");
    });
  });
});
