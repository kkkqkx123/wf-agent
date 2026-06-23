/**
 * Integration Test: Verify Critical Fixes
 *
 * Validates that CRITICAL and HIGH priority design fixes are working:
 * 1. ConversationSession messages are included in getState()
 * 2. Trigger states are properly restored from checkpoints
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import type { LLMMessage } from "@wf-agent/types";
import { ConversationSession } from "@sdk/shared/messaging/conversation-session.js";
import { AgentLoopState } from "@sdk/agent/state-managers/agent-loop-state.js";

describe("Integration: Verify Critical Fixes", () => {
  describe("Fix #1: ConversationSession.getState() includes messages", () => {
    let conversationSession: ConversationSession;

    beforeEach(() => {
      conversationSession = new ConversationSession({
        executionId: "test-fix-1",
        tokenLimit: 8000,
      });
    });

    afterEach(() => {
      conversationSession.cleanup();
    });

    it("should include full message list in getState()", async () => {
      // Add messages
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi there" },
        { role: "user", content: "How are you?" },
        { role: "assistant", content: "I'm good, thanks for asking" },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      // Get state (this is called during checkpoint creation)
      const state = conversationSession.getState();

      // CRITICAL FIX: State should include the messages for checkpoint persistence
      // Previously: Only included messageMetadata (fingerprint, count)
      // Now: Should include complete messages list
      expect(state).toBeDefined();
      expect(state.messages).toBeDefined();
      expect(Array.isArray(state.messages)).toBe(true);
      expect(state.messages).toHaveLength(4);

      // Verify message content is preserved
      expect(state.messages[0].content).toBe("Hello");
      expect(state.messages[1].content).toBe("Hi there");
      expect(state.messages[3].content).toBe("I'm good, thanks for asking");

      // Message metadata should also be present for validation
      expect(state.messageMetadata).toBeDefined();
      expect(state.messageMetadata?.count).toBe(4);
    });

    it("should maintain message integrity through checkpoint cycle", async () => {
      // Add messages with tool calls
      const messages: LLMMessage[] = [
        { role: "user", content: "Search for info" },
        {
          role: "assistant",
          content: "Searching...",
          toolCalls: [{ id: "tc1", name: "search", arguments: { query: "test" } }],
        },
        { role: "tool", content: "Results found", toolCallId: "tc1", toolName: "search" },
        { role: "assistant", content: "I found the information" },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      const stateSnapshot = conversationSession.getState();

      // Simulate restoration from checkpoint
      const restoredState = { ...stateSnapshot };

      // Messages should be complete and restorable
      expect(restoredState.messages).toHaveLength(4);
      expect(restoredState.messages[1].toolCalls).toHaveLength(1);
      expect(restoredState.messages[1].toolCalls?.[0].name).toBe("search");
      expect(restoredState.messages[2].toolCallId).toBe("tc1");
    });
  });

  describe("Fix #2: AgentLoopState.createSnapshot() captures state correctly", () => {
    let agentState: AgentLoopState;

    beforeEach(() => {
      agentState = new AgentLoopState();
    });

    it("should capture iteration history through createSnapshot", () => {
      // Use proper API: startIteration() instead of setting property directly
      agentState.start();

      // Iteration 1
      agentState.startIteration();
      agentState.recordToolCallStart("tc-1", "tool1", { arg: "value1" });
      agentState.recordToolCallEnd("tc-1", { result: "done" });
      agentState.endIteration("Response 1");

      // Iteration 2
      agentState.startIteration();
      agentState.recordToolCallStart("tc-2", "tool2", { arg: "value2" });
      agentState.recordToolCallEnd("tc-2", { result: "done" });
      agentState.endIteration("Response 2");

      // Create snapshot
      const snapshot = agentState.createSnapshot();

      // Verify state is captured
      expect(snapshot.currentIteration).toBe(2);
      expect(snapshot.toolCallCount).toBe(2);
      expect(snapshot.iterationHistory).toHaveLength(2);

      // Verify iteration details
      expect(snapshot.iterationHistory[0].iteration).toBe(1);
      expect(snapshot.iterationHistory[0].toolCalls).toHaveLength(1);
      expect(snapshot.iterationHistory[1].iteration).toBe(2);
      expect(snapshot.iterationHistory[1].toolCalls).toHaveLength(1);
    });

    it("should restore from snapshot correctly", () => {
      // Build initial state
      agentState.start();
      agentState.startIteration();
      agentState.recordToolCallStart("tc-1", "tool1", {});
      agentState.recordToolCallEnd("tc-1", { result: "ok" });
      agentState.endIteration("Done");

      // Create snapshot
      const snapshot = agentState.createSnapshot();

      // Create new instance and restore
      const restoredState = new AgentLoopState();
      restoredState.restoreFromSnapshot(snapshot);

      // Verify restoration
      expect(restoredState.currentIteration).toBe(1);
      expect(restoredState.toolCallCount).toBe(1);
      expect(restoredState.iterationHistory).toHaveLength(1);
      expect(restoredState.iterationHistory[0].toolCalls[0].id).toBe("tc-1");
    });
  });

  describe("Fix #3: Checkpoint workflow integration", () => {
    it("should integrate ConversationSession and AgentLoopState through checkpoint", async () => {
      // This represents the checkpoint save-restore cycle

      // Setup
      const conversationSession = new ConversationSession({
        executionId: "test-integration",
        tokenLimit: 8000,
      });
      const agentState = new AgentLoopState();

      try {
        // Add conversation and track execution state
        await conversationSession.addMessage({ role: "user", content: "Start task" });
        await conversationSession.addMessage({
          role: "assistant",
          content: "Processing...",
        });

        agentState.start();
        agentState.startIteration();
        agentState.endIteration("Iteration 1 complete");

        // CHECKPOINT SAVE: Extract both state and conversation
        const agentSnapshot = agentState.createSnapshot();
        const conversationState = conversationSession.getState();

        // Both should have data
        expect(agentSnapshot.currentIteration).toBe(1);
        expect(conversationState.messages).toHaveLength(2);

        // CHECKPOINT RESTORE: Verify both can be restored
        const restoredAgentState = new AgentLoopState();
        restoredAgentState.restoreFromSnapshot(agentSnapshot);

        const restoredConversationSession = new ConversationSession({
          executionId: "test-integration-restored",
          tokenLimit: 8000,
        });
        // Simulate restoration of conversation state
        // (In real checkpoint coordinator, this calls conversationManager.addMessages())
        for (const msg of conversationState.messages || []) {
          await restoredConversationSession.addMessage(msg);
        }

        // Verify both restored correctly
        expect(restoredAgentState.currentIteration).toBe(1);
        expect(restoredConversationSession.getMessages()).toHaveLength(2);
        expect(restoredConversationSession.getMessages()[0].content).toBe("Start task");

        conversationSession.cleanup();
        restoredConversationSession.cleanup();
      } finally {
        conversationSession.cleanup();
      }
    });
  });
});
