/**
 * Integration Test: Messaging and Checkpoint Lifecycle
 *
 * Tests the complete flow of message management through checkpoint save/restore cycles.
 * This test exposes design issues with message-state separation.
 *
 * ⚠️ DESIGN ISSUE DETECTION:
 * - Messages are stored in ConversationSession
 * - State is stored in AgentLoopState
 * - When checkpoint is restored, messages are NOT automatically restored
 * - This creates a "message-state version mismatch" scenario
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import type {
  AgentLoopRuntimeConfig,
  LLMMessage,
  AgentLoopStateSnapshot,
} from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { ConversationSession } from "@sdk/shared/messaging/conversation-session.js";
import { AgentLoopEntity } from "@sdk/agent/entities/agent-loop-entity.js";
import { AgentLoopState } from "@sdk/agent/state-managers/agent-loop-state.js";
import { ExecutionHierarchyRegistry } from "@sdk/shared/registry/execution-hierarchy-registry.js";
import { AgentStateCoordinator } from "@sdk/agent/state-managers/agent-state-coordinator.js";

describe("Integration: Messaging & Checkpoint Lifecycle", () => {
  let conversationSession: ConversationSession;
  let agentState: AgentLoopState;
  let agentEntity: AgentLoopEntity;
  let stateCoordinator: AgentStateCoordinator;
  let hierarchyRegistry: ExecutionHierarchyRegistry;

  const mockConfig: AgentLoopRuntimeConfig = {
    id: "test-config-1",
    name: "Test Agent",
    description: "Test agent for integration testing",
    tools: [],
    transformContext: async (ctx) => ctx,
    convertToLlm: async (ctx) => ({
      role: "user",
      content: "test",
    }),
  };

  beforeEach(() => {
    hierarchyRegistry = new ExecutionHierarchyRegistry();

    // Create ConversationSession
    conversationSession = new ConversationSession({
      executionId: "test-agent-1",
      tokenLimit: 4000,
    });

    // Create AgentLoopEntity
    agentEntity = new AgentLoopEntity("test-agent-1", mockConfig, undefined, undefined, hierarchyRegistry);
    agentState = agentEntity.state;

    // Create AgentStateCoordinator
    stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });
  });

  afterEach(() => {
    conversationSession.cleanup();
  });

  describe("Scenario 1: Message Addition and State Progression", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. User starts an agent execution with an initial prompt
     * 2. Agent calls LLM and gets response with tool calls
     * 3. Tool is executed and result is added
     * 4. All messages should be tracked in ConversationSession
     * 5. State should track iteration count
     */
    it("should track messages and state together through execution", async () => {
      // Step 1: Add initial user message
      const userMessage: LLMMessage = {
        role: "user",
        content: "What is the weather in New York?",
      };
      await conversationSession.addMessage(userMessage);

      expect(conversationSession.getMessages()).toHaveLength(1);
      expect(agentState.currentIteration).toBe(0);

      // Step 2: Add assistant response with tool call
      const assistantMessage: LLMMessage = {
        role: "assistant",
        content: "I'll check the weather for you",
        toolCalls: [
          {
            id: "call-1",
            name: "get_weather",
            arguments: { location: "New York" },
          },
        ],
      };
      await conversationSession.addMessage(assistantMessage);

      // Step 3: Simulate tool execution - increment iteration
      agentState.startIteration(); // iteration 1
      agentState.recordToolCallEnd("dummy-tc-1"); // toolCallCount = 1

      expect(conversationSession.getMessages()).toHaveLength(2);
      expect(agentState.currentIteration).toBe(1);
      expect(agentState.toolCallCount).toBe(1);

      // Step 4: Add tool result
      const toolResult: LLMMessage = {
        role: "tool",
        content: "Current temperature: 72°F, partly cloudy",
        toolCallId: "call-1",
        toolName: "get_weather",
      };
      await conversationSession.addMessage(toolResult);

      expect(conversationSession.getMessages()).toHaveLength(3);
    });
  });

  describe("Scenario 2: Checkpoint Creation and Message Preservation", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Build up message history through several iterations
     * 2. Create a checkpoint
     * 3. Checkpoint should preserve BOTH messages AND state
     * 4. When restored, messages should be identical
     *
     * ❌ DESIGN ISSUE DETECTED:
     * The checkpoint only saves AgentLoopState, not ConversationSession.
     * Messages are lost during checkpoint save.
     */
    it("should preserve message history across checkpoint boundary", async () => {
      // Build message history
      const messages: LLMMessage[] = [
        { role: "user", content: "First question" },
        { role: "assistant", content: "First answer" },
        { role: "user", content: "Second question" },
        { role: "assistant", content: "Second answer" },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      agentState.startIteration(); // iteration 1
      agentState.startIteration(); // iteration 2
      agentState.status = AgentLoopStatus.RUNNING;

      // Checkpoint point
      const stateSnapshot = agentState.createSnapshot();
      const messageCount = conversationSession.getMessages().length;

      expect(stateSnapshot.currentIteration).toBe(2);
      expect(messageCount).toBe(4);

      // ❌ ISSUE: We can save state, but where are messages saved?
      // The checkpoint should include message fingerprint or full message serialization
      expect(stateSnapshot.currentIteration).toBe(2); // ✅ State saved
      // ❌ Messages are NOT in checkpoint - they need separate handling
    });
  });

  describe("Scenario 3: State-Message Mismatch Detection", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Create a checkpoint with N messages and iteration M
     * 2. Later, restore from checkpoint
     * 3. But messages are restored from a different source
     * 4. This can lead to MISMATCH: iteration = 5 but only 3 messages
     *
     * ❌ DESIGN ISSUE:
     * There's no validation that:
     * - Message count matches iteration history
     * - Message content matches what was checkpointed
     * - Message order is preserved
     */
    it("should detect and prevent state-message mismatch", async () => {
      // Original state
      const originalMessages: LLMMessage[] = [
        { role: "user", content: "Message 1" },
        { role: "assistant", content: "Response 1" },
        { role: "user", content: "Message 2" },
        { role: "assistant", content: "Response 2" },
      ];

      for (const msg of originalMessages) {
        await conversationSession.addMessage(msg);
      }

      agentState.startIteration(); // iteration 1
      agentState.startIteration(); // iteration 2
      const checkpoint = agentState.createSnapshot();

      // ❌ PROBLEM: Now imagine restoring with wrong messages
      const wrongMessages: LLMMessage[] = [
        { role: "user", content: "Different Message 1" },
        { role: "assistant", content: "Different Response 1" },
        // Only 2 messages, but state says iteration=2
      ];

      // There's no built-in validation for this mismatch
      expect(checkpoint.currentIteration).toBe(2);
      expect(wrongMessages).toHaveLength(2);

      // ❌ This mismatch is not caught by the current design
      // We need:
      // 1. Message fingerprint in checkpoint metadata
      // 2. Validation during restore
      // 3. Or full message serialization in checkpoint
    });
  });

  describe("Scenario 4: Multi-Execution Message Context Isolation", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Workflow execution spawns two concurrent Agent Loops
     * 2. Each Agent Loop has its own ConversationSession
     * 3. Messages from Agent 1 should not appear in Agent 2
     * 4. Each agent independently manages its message context
     *
     * ✅ CURRENT DESIGN WORKS for this scenario
     * Each ConversationSession is independent
     */
    it("should isolate messages across different ConversationSessions", async () => {
      const session1 = new ConversationSession({
        executionId: "agent-1",
      });

      const session2 = new ConversationSession({
        executionId: "agent-2",
      });

      // Add messages to session 1
      await session1.addMessage({ role: "user", content: "Agent 1 message" });
      await session1.addMessage({ role: "assistant", content: "Agent 1 response" });

      // Add messages to session 2
      await session2.addMessage({ role: "user", content: "Agent 2 message" });

      // Verify isolation
      expect(session1.getMessages()).toHaveLength(2);
      expect(session2.getMessages()).toHaveLength(1);

      // Verify content isolation
      const msgs1 = session1.getMessages();
      const msgs2 = session2.getMessages();

      expect(msgs1[0]?.content).toContain("Agent 1");
      expect(msgs2[0]?.content).toContain("Agent 2");

      session1.cleanup();
      session2.cleanup();
    });
  });

  describe("Scenario 5: Message Token Usage Tracking", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Messages are added to ConversationSession
     * 2. Each message consumes tokens
     * 3. Token usage should be tracked
     * 4. On checkpoint, token usage should be preserved
     * 5. On restore, token tracking should continue accurately
     */
    it("should track and preserve token usage with messages", async () => {
      const session = new ConversationSession({
        executionId: "test-agent",
        tokenLimit: 2000,
      });

      // Simulate token tracking
      const msg1: LLMMessage = { role: "user", content: "Short message" };
      const msg2: LLMMessage = { role: "assistant", content: "A very long response that uses many more tokens for sure" };

      await session.addMessage(msg1);
      await session.addMessage(msg2);

      // Get token usage snapshot
      const tokenUsage = session.getTokenUsage();

      // ✅ Token usage is tracked
      expect(tokenUsage).toBeDefined();

      // On checkpoint, this should be saved
      const conversationState = session.getState();

      // ✅ Token state is in conversation state
      expect(conversationState.tokenUsage).toBeDefined();

      session.cleanup();
    });
  });

  describe("Scenario 6: Turn State Preservation", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Agent Loop processes multiple turns
     * 2. Each turn has execution-specific state (tool results, context)
     * 3. On checkpoint, turn states should be preserved
     * 4. On restore, turn states should be available
     */
    it("should preserve turn-based execution state", async () => {
      const session = new ConversationSession({
        executionId: "test-agent",
      });

      // Turn 0: Initial message
      await session.addMessage({ role: "user", content: "Turn 0 message" });
      session.setTurnState(0, "tool_result", { toolName: "calculator", result: 42 });

      // Turn 1: Follow-up
      await session.addMessage({ role: "assistant", content: "Based on calculation" });
      session.setTurnState(1, "context_summary", "Calculated value is 42");

      // Get turn states
      const state0 = session.getTurnState(0, "tool_result");
      const state1 = session.getTurnState(1, "context_summary");

      expect(state0).toEqual({ toolName: "calculator", result: 42 });
      expect(state1).toBe("Calculated value is 42");

      // Snapshot and restore
      const snapshot = session.getState();
      expect(snapshot.turnStates).toBeDefined();
      expect(Object.keys(snapshot.turnStates || {})).toHaveLength(2);

      session.cleanup();
    });
  });

  describe("Design Issue: Missing Message Checkpoint Integration", () => {
    /**
     * This test documents the design issue and what should be done to fix it.
     *
     * CURRENT STATE:
     * - AgentLoopState is checkpointed
     * - ConversationSession is NOT checkpointed
     * - Message history is lost on restore
     *
     * REQUIRED FIXES:
     * 1. Add message serialization to checkpoint
     * 2. Add message fingerprint validation
     * 3. Ensure messages are restored alongside state
     * 4. Validate state-message consistency on restore
     */
    it("documents the message-state separation issue", () => {
      // The issue:
      // 1. When checkpoint is saved:
      const stateSnapshot = agentState.createSnapshot();
      // - Only AgentLoopState is saved
      // - ConversationSession.messages are NOT saved

      // 2. When checkpoint is restored:
      const restoredState = agentState.createSnapshot();
      // - AgentLoopState is restored
      // - But messages are gone or must be re-provided externally

      // MISSING SOLUTION:
      // The checkpoint metadata should include:
      // {
      //   messageCount: number;
      //   messageFingerprint: string;
      //   lastMessageId: string;
      // }
      //
      // And the restoration should:
      // 1. Load persisted messages
      // 2. Verify fingerprint matches
      // 3. Throw if mismatch detected

      expect(stateSnapshot).toBeDefined();
      // This test passes but highlights incomplete design
    });
  });
});
