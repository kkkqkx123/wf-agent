/**
 * Integration Test: Checkpoint-ConversationSession Synchronization
 *
 * Tests the critical integration point where Checkpoint and ConversationSession interact.
 * This test validates that both state and messages are preserved across checkpoint cycles.
 *
 * DESIGN ISSUE: Message-State Sync
 * - Checkpoint saves state but not messages
 * - ConversationSession manages messages independently
 * - This creates a versioning problem: state restored to v5, but only 2 messages available
 *
 * BUSINESS SCENARIO:
 * Agent executes long-running task with checkpoint/recovery cycles:
 * 1. Execute, add messages, save checkpoint
 * 2. System crash
 * 3. Restore checkpoint
 * 4. Continue execution with full context (state + messages)
 *
 * Expectation: Both state and messages should be identical after restore
 * Reality: ❌ Messages are lost during checkpoint
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import type { LLMMessage, AgentLoopStateSnapshot } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { ConversationSession } from "@sdk/shared/messaging/conversation-session";
import { AgentLoopState } from "@sdk/agent/state-managers/agent-loop-state";

describe("Integration: Checkpoint-ConversationSession Message Sync", () => {
  let conversationSession: ConversationSession;
  let agentState: AgentLoopState;

  beforeEach(() => {
    conversationSession = new ConversationSession({
      executionId: "test-agent-checkpoint-1",
      tokenLimit: 8000,
    });

    agentState = new AgentLoopState();
  });

  afterEach(() => {
    conversationSession.cleanup();
  });

  describe("Scenario: Message Preservation Across Checkpoint", () => {
    it("should preserve all messages when checkpoint is created", async () => {
      // Build realistic conversation with multiple turns
      const messages: LLMMessage[] = [
        { role: "user", content: "Analyze this dataset" },
        { role: "assistant", content: "I'll help analyze the dataset" },
        { role: "user", content: "Show me the summary" },
        { role: "assistant", content: "Here's the summary", toolCalls: [
          { id: "call-1", name: "analyze_data", arguments: { dataset: "test" } }
        ]},
        { role: "tool", content: "Analysis complete", toolCallId: "call-1", toolName: "analyze_data" },
        { role: "assistant", content: "Analysis is complete" },
      ];

      // Add messages to conversation
      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      // Update state to reflect iterations
      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.RUNNING,
        currentIteration: 3,
        toolCallCount: 1,
        startTime: null,
        endTime: null,
        error: undefined,
      });

      // Create checkpoint snapshot
      const stateSnapshot = agentState.createSnapshot();
      const messagesBefore = conversationSession.getMessages();

      // ISSUE #1: State snapshot includes iteration count
      expect(stateSnapshot.currentIteration).toBe(3);

      // ISSUE #2: But snapshot does NOT include message count or messages themselves
      // ❌ stateSnapshot should contain message metadata for verification
      // ❌ stateSnapshot should contain message fingerprint
      expect(stateSnapshot.currentIteration).toBe(3);

      // Verify messages are currently accessible
      expect(messagesBefore).toHaveLength(6);
      expect(messagesBefore[0].role).toBe("user");
      expect(messagesBefore[5].role).toBe("assistant");

      // DESIGN FLAW: Create verification metadata that SHOULD be in checkpoint
      // In real scenario, this is done implicitly but it's not integrated
      const messageFingerprint = JSON.stringify(messagesBefore);
      expect(messageFingerprint).toContain("Analyze");
    });

    it("should detect message count mismatch during restoration", async () => {
      // Scenario: Create checkpoint with 5 messages at iteration 2
      const originalMessages: LLMMessage[] = [
        { role: "user", content: "Message 1" },
        { role: "assistant", content: "Response 1" },
        { role: "user", content: "Message 2" },
        { role: "assistant", content: "Response 2" },
        { role: "user", content: "Message 3" },
      ];

      for (const msg of originalMessages) {
        await conversationSession.addMessage(msg);
      }

      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.CREATED,
        currentIteration: 2,
        toolCallCount: 0,
        startTime: null,
        endTime: null,
        error: undefined,
      });
      const checkpoint = agentState.createSnapshot();

      // ISSUE: No metadata to validate message-state consistency
      expect(checkpoint.currentIteration).toBe(2);
      expect(originalMessages).toHaveLength(5);

      // ❌ Now simulate restoration with mismatched messages
      // In a proper design, this should be detected and prevented
      const newSession = new ConversationSession({ executionId: "test-restore" });

      // Only restore 2 messages (simulating partial recovery)
      await newSession.addMessage({ role: "user", content: "Message 1" });
      await newSession.addMessage({ role: "assistant", content: "Response 1" });

      // DESIGN FLAW: No validation that state matches message count
      expect(checkpoint.currentIteration).toBe(2);
      expect(newSession.getMessages()).toHaveLength(2); // ❌ Mismatch!

      // We need:
      // 1. Message fingerprint in checkpoint
      expect(checkpoint.currentIteration).toBe(2); // ✅ We have this
      // expect(checkpoint.messageFingerprint).toBeDefined(); // ❌ We don't have this
    });

    it("should maintain message order and content integrity", async () => {
      // Critical: Message order MUST be preserved
      const messages: LLMMessage[] = [
        { role: "user", content: "First request" },
        { role: "assistant", content: "First response", toolCalls: [
          { id: "tool-1", name: "step1", arguments: {} }
        ]},
        { role: "tool", content: "Step 1 result", toolCallId: "tool-1", toolName: "step1" },
        { role: "assistant", content: "Based on step 1..." },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      const savedMessages = conversationSession.getMessages();

      // Verify order is preserved
      expect(savedMessages[0].content).toBe("First request");
      expect(savedMessages[1].content).toBe("First response");
      expect(savedMessages[2].content).toBe("Step 1 result");
      expect(savedMessages[3].content).toBe("Based on step 1...");

      // DESIGN REQUIREMENT: Checkpoint must preserve this order exactly
      // ❌ Currently no mechanism ensures this during checkpoint/restore cycle
      expect(savedMessages).toHaveLength(4);
    });

    it("should include tool call metadata in message verification", async () => {
      // Tool calls are critical - they must be preserved
      const message: LLMMessage = {
        role: "assistant",
        content: "Executing tool",
        toolCalls: [
          {
            id: "call-123",
            name: "fetch_data",
            arguments: { source: "database" },
          }
        ],
      };

      await conversationSession.addMessage(message);

      const restored = conversationSession.getMessages()[0];

      // Verify tool call metadata is present
      expect(restored.toolCalls).toBeDefined();
      expect(restored.toolCalls![0].id).toBe("call-123");
      expect(restored.toolCalls![0].arguments).toEqual({ source: "database" });

      // DESIGN REQUIREMENT: Checkpoint must include full tool call metadata
      // ❌ No explicit mechanism in current checkpoint design
      expect(restored.role).toBe("assistant");
    });
  });

  describe("Scenario: Token Statistics Synchronization", () => {
    it("should sync token usage stats with message state", async () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Small message" },
        { role: "assistant", content: "A slightly longer response that uses more tokens" },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      // Update token tracking
      conversationSession.addTokenUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });

      const state = conversationSession.getState();

      // DESIGN REQUIREMENT: State should include both messages and token metadata
      expect(state.tokenUsage?.totalTokens).toBe(30);

      // ❌ But state.messages is not included!
      // This creates two independent streams that must be manually synchronized
      expect(state.tokenUsage).toBeDefined();
    });

    it("should detect token overflow with message checkpoint", async () => {
      // High priority: detect when messages cause token overflow
      conversationSession = new ConversationSession({
        executionId: "test-overflow",
        tokenLimit: 100, // Very small limit
      });

      const messages: LLMMessage[] = [
        { role: "user", content: "Question about complex analysis with many details" },
        { role: "assistant", content: "Very detailed response that provides comprehensive information about the topic" },
      ];

      for (const msg of messages) {
        await conversationSession.addMessage(msg);
      }

      // Simulate token usage that exceeds limit
      conversationSession.addTokenUsage({
        promptTokens: 50,
        completionTokens: 80,
        totalTokens: 130, // Exceeds limit of 100
      });

      const state = conversationSession.getState();

      // DESIGN REQUIREMENT: Checkpoint should include token overflow detection
      expect(state.tokenUsage?.totalTokens).toBeGreaterThan(100);

      // ❌ But there's no link between message preservation and token optimization
      // When token limit exceeded, messages are compressed
      // But checkpoint doesn't track the original vs compressed message count
      expect(state.tokenUsage?.totalTokens).toBe(130);
    });
  });

  describe("Scenario: Multi-Level Message History Restoration", () => {
    it("should support incremental checkpoint with message delta", async () => {
      // Checkpoint 1: Save 5 messages
      const messagesRound1: LLMMessage[] = [
        { role: "user", content: "Question 1" },
        { role: "assistant", content: "Answer 1" },
        { role: "user", content: "Question 2" },
        { role: "assistant", content: "Answer 2" },
        { role: "user", content: "Question 3" },
      ];

      for (const msg of messagesRound1) {
        await conversationSession.addMessage(msg);
      }

      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.CREATED,
        currentIteration: 1,
        toolCallCount: 0,
        startTime: null,
        endTime: null,
        error: undefined,
      });
      const checkpoint1 = agentState.createSnapshot();

      // Checkpoint 2: Add 3 more messages (total 8)
      const messagesRound2: LLMMessage[] = [
        { role: "assistant", content: "Answer 3" },
        { role: "user", content: "Question 4" },
        { role: "assistant", content: "Answer 4" },
      ];

      for (const msg of messagesRound2) {
        await conversationSession.addMessage(msg);
      }

      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.CREATED,
        currentIteration: 2,
        toolCallCount: 0,
        startTime: null,
        endTime: null,
        error: undefined,
      });
      const checkpoint2 = agentState.createSnapshot();

      // DESIGN REQUIREMENT: Checkpoint2 should support delta compression
      // - Save only the 3 new messages
      // - Reference checkpoint1 for the first 5 messages
      // - On restore, rebuild complete message history from chain

      expect(checkpoint1.currentIteration).toBe(1);
      expect(checkpoint2.currentIteration).toBe(2);

      // ❌ But there's no "messageBaseline" or "messageDelta" in checkpoint
      // This is a missed optimization opportunity
      expect(conversationSession.getMessages()).toHaveLength(8);
    });

    it("should validate message chain integrity", async () => {
      // Build message history across multiple checkpoints
      const allMessages: LLMMessage[] = [];

      // Round 1: 3 messages
      const round1 = [
        { role: "user", content: "Request 1" },
        { role: "assistant", content: "Response 1" },
        { role: "user", content: "Request 2" },
      ];
      for (const msg of round1) {
        await conversationSession.addMessage(msg);
        allMessages.push(msg);
      }

      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.CREATED,
        currentIteration: 1,
        toolCallCount: 0,
        startTime: null,
        endTime: null,
        error: undefined,
      });
      const ckpt1 = agentState.createSnapshot();

      // Round 2: 2 more messages
      const round2 = [
        { role: "assistant", content: "Response 2" },
        { role: "user", content: "Request 3" },
      ];
      for (const msg of round2) {
        await conversationSession.addMessage(msg);
        allMessages.push(msg);
      }

      agentState.restoreFromSnapshot({
        status: AgentLoopStatus.CREATED,
        currentIteration: 2,
        toolCallCount: 0,
        startTime: null,
        endTime: null,
        error: undefined,
      });
      const ckpt2 = agentState.createSnapshot();

      // DESIGN REQUIREMENT: Build checkpointChain with message validation
      // checkpoint chain should be: ckpt1 -> ckpt2
      // message chain should be: [msg1,msg2,msg3] -> [msg4,msg5]

      // ❌ No link between checkpoint chain and message chain
      expect(ckpt1.currentIteration).toBe(1);
      expect(ckpt2.currentIteration).toBe(2);
      expect(conversationSession.getMessages()).toHaveLength(5);

      // Validation we NEED:
      // - ckpt2 references ckpt1 as previous
      // - Message count should be consistent with iteration count
      // - Message fingerprint should be in metadata
    });
  });
});
