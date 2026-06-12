/**
 * ConversationCoordinator Unit Tests
 *
 * Tests for conversation management coordination:
 * - Getting conversation managers
 * - Normalizing conversation history
 * - Retrieving conversation statistics
 * - Handling missing agent loops
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentLoopRegistry } from "../../../stores/agent-loop-registry.js";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import { ConversationCoordinator } from "../conversation-coordinator.js";

describe("ConversationCoordinator", () => {
  let coordinator: ConversationCoordinator;
  let mockRegistry: AgentLoopRegistry;
  let mockConversationManager: ConversationSession;

  beforeEach(() => {
    mockConversationManager = {
      getMessages: vi.fn(),
      getTokenUsage: vi.fn(),
    } as unknown as ConversationSession;

    mockRegistry = {
      get: vi.fn(),
    } as unknown as AgentLoopRegistry;

    coordinator = new ConversationCoordinator(mockRegistry);
  });

  describe("getConversationManager", () => {
    it("should return conversation manager when agent loop exists", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(mockConversationManager),
      });

      const result = await coordinator.getConversationManager("agent-loop-1");

      expect(result).toBe(mockConversationManager);
      expect(mockRegistry.get).toHaveBeenCalledWith("agent-loop-1");
    });

    it("should return undefined when agent loop is not found", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const result = await coordinator.getConversationManager("non-existent");

      expect(result).toBeUndefined();
    });
  });

  describe("getNormalizedHistory", () => {
    it("should return messages when conversation manager exists", async () => {
      const mockMessages = [
        { role: "system", content: "You are helpful" },
        { role: "user", content: "Hello" },
      ];
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(mockConversationManager),
      });
      (mockConversationManager.getMessages as ReturnType<typeof vi.fn>).mockReturnValue(
        mockMessages,
      );

      const result = await coordinator.getNormalizedHistory("agent-loop-1");

      expect(result).toEqual(mockMessages);
      expect(result.length).toBe(2);
    });

    it("should return empty array when agent loop not found", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const result = await coordinator.getNormalizedHistory("non-existent");

      expect(result).toEqual([]);
    });

    it("should return empty array when conversation manager is undefined", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(undefined),
      });

      const result = await coordinator.getNormalizedHistory("agent-loop-1");

      expect(result).toEqual([]);
    });
  });

  describe("getConversationStats", () => {
    it("should return stats with role distribution and token usage", async () => {
      const mockMessages = [
        { role: "system", content: "You are helpful" },
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi!" },
        { role: "user", content: "How are you?" },
      ];
      const mockTokenUsage = { prompt: 100, completion: 50, total: 150 };

      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(mockConversationManager),
      });
      (mockConversationManager.getMessages as ReturnType<typeof vi.fn>).mockReturnValue(
        mockMessages,
      );
      (mockConversationManager.getTokenUsage as ReturnType<typeof vi.fn>).mockReturnValue(
        mockTokenUsage,
      );

      const result = await coordinator.getConversationStats("agent-loop-1");

      expect(result).toEqual({
        totalMessages: 4,
        roleDistribution: {
          system: 1,
          user: 2,
          assistant: 1,
        },
        totalTokenUsage: mockTokenUsage,
      });
    });

    it("should return undefined when agent loop not found", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const result = await coordinator.getConversationStats("non-existent");

      expect(result).toBeUndefined();
    });

    it("should return undefined when conversation manager is undefined", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(undefined),
      });

      const result = await coordinator.getConversationStats("agent-loop-1");

      expect(result).toBeUndefined();
    });

    it("should handle empty messages array", async () => {
      (mockRegistry.get as ReturnType<typeof vi.fn>).mockResolvedValue({
        getConversationManager: vi.fn().mockReturnValue(mockConversationManager),
      });
      (mockConversationManager.getMessages as ReturnType<typeof vi.fn>).mockReturnValue([]);
      (mockConversationManager.getTokenUsage as ReturnType<typeof vi.fn>).mockReturnValue({
        prompt: 0,
        completion: 0,
        total: 0,
      });

      const result = await coordinator.getConversationStats("agent-loop-1");

      expect(result).toEqual({
        totalMessages: 0,
        roleDistribution: {},
        totalTokenUsage: { prompt: 0, completion: 0, total: 0 },
      });
    });
  });
});
