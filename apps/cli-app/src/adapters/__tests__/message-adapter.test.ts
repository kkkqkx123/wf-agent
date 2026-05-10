/**
 * Message Adapter Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { MessageAdapter } from "../message-adapter.js";
import { success, failure } from "@wf-agent/sdk";

// Mock SDK dependencies
vi.mock("@wf-agent/sdk", async () => {
  const actual = await vi.importActual("@wf-agent/sdk");
  return {
    ...actual,
  };
});

vi.mock("../../src/index.js", () => ({
  getSDKInstance: vi.fn(),
}));

vi.mock("../../src/utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    success: vi.fn(),
    fail: vi.fn(),
    result: vi.fn(),
    errorResult: vi.fn(),
    debugLog: vi.fn(),
  })),
}));

describe("MessageAdapter", () => {
  let adapter: MessageAdapter;
  let mockSdk: any;
  let mockMessagesApi: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Mock messages API
    mockMessagesApi = {
      getAll: vi.fn(),
      get: vi.fn(),
      getMessageStats: vi.fn(),
      getGlobalMessageStats: vi.fn(),
    };

    // Mock SDK instance
    mockSdk = {
      messages: mockMessagesApi,
    };

    // Setup getSDKInstance mock
    const { getSDKInstance } = require("../../src/index.js");
    getSDKInstance.mockReturnValue(mockSdk);

    // Create adapter instance
    adapter = new MessageAdapter();
  });

  describe("listMessages", () => {
    it("should list all messages", async () => {
      const mockMessages = [
        { id: "msg-1", role: "user", content: "Hello" },
        { id: "msg-2", role: "assistant", content: "Hi there" },
      ];
      mockMessagesApi.getAll.mockResolvedValue(success(mockMessages, 25));

      const result = await adapter.listMessages();

      expect(mockMessagesApi.getAll).toHaveBeenCalled();
      expect(result).toHaveLength(2);
      expect(result[0]).toHaveProperty("id", "msg-1");
      expect(result[0]).toHaveProperty("role", "user");
    });

    it("should list messages with filter", async () => {
      const mockMessages = [{ id: "msg-1", role: "user", content: "Hello" }];
      const filter = { role: "user" };
      mockMessagesApi.getAll.mockResolvedValue(success(mockMessages, 20));

      const result = await adapter.listMessages(filter);

      expect(mockMessagesApi.getAll).toHaveBeenCalledWith(filter);
      expect(result).toHaveLength(1);
    });

    it("should handle list failure", async () => {
      const mockError = new Error("List failed");
      mockMessagesApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listMessages()).rejects.toThrow("List failed");
    });
  });

  describe("getMessage", () => {
    it("should get message by ID", async () => {
      const mockMessage = { id: "msg-123", role: "user", content: "Test message" };
      mockMessagesApi.get.mockResolvedValue(success(mockMessage, 15));

      const result = await adapter.getMessage("msg-123");

      expect(mockMessagesApi.get).toHaveBeenCalledWith("msg-123");
      expect(result).toEqual(mockMessage);
    });

    it("should throw error when message not found", async () => {
      mockMessagesApi.get.mockResolvedValue(success(null, 15));

      await expect(adapter.getMessage("non-existent")).rejects.toThrow(
        "Message not found: non-existent",
      );
    });

    it("should handle get failure", async () => {
      const mockError = new Error("Get failed");
      mockMessagesApi.get.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.getMessage("msg-123")).rejects.toThrow("Get failed");
    });
  });

  describe("listMessagesByExecution", () => {
    it("should list messages by execution ID", async () => {
      const mockMessages = [
        { id: "msg-1", role: "user", content: "First message" },
        { id: "msg-2", role: "assistant", content: "Response" },
      ];
      mockMessagesApi.getAll.mockResolvedValue(success(mockMessages, 20));

      const result = await adapter.listMessagesByExecution("exec-123");

      expect(mockMessagesApi.getAll).toHaveBeenCalledWith({ executionId: "exec-123" });
      expect(result).toHaveLength(2);
    });

    it("should handle list failure", async () => {
      const mockError = new Error("List failed");
      mockMessagesApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listMessagesByExecution("exec-123")).rejects.toThrow("List failed");
    });
  });

  describe("getMessageStats", () => {
    it("should get message statistics for a specific execution", async () => {
      const mockStats = {
        total: 50,
        byRole: {
          user: 25,
          assistant: 20,
          system: 5,
        },
        byType: {
          text: 45,
          object: 5,
        },
      };
      mockMessagesApi.getMessageStats.mockResolvedValue(mockStats);

      const result = await adapter.getMessageStats("exec-123");

      expect(mockMessagesApi.getMessageStats).toHaveBeenCalledWith("exec-123");
      expect(result.total).toBe(50);
      expect(result.byRole).toEqual(mockStats.byRole);
      expect(result.byType).toEqual(mockStats.byType);
    });

    it("should handle stats retrieval failure", async () => {
      const mockError = new Error("Stats retrieval failed");
      mockMessagesApi.getMessageStats.mockRejectedValue(mockError);

      await expect(adapter.getMessageStats("exec-123")).rejects.toThrow("Stats retrieval failed");
    });
  });

  describe("getGlobalMessageStats", () => {
    it("should get global message statistics across all executions", async () => {
      const mockStats = {
        total: 100,
        byExecution: {
          "exec-1": 50,
          "exec-2": 30,
          "exec-3": 20,
        },
        byRole: {
          user: 50,
          assistant: 45,
          system: 5,
        },
      };
      mockMessagesApi.getGlobalMessageStats.mockResolvedValue(mockStats);

      const result = await adapter.getGlobalMessageStats();

      expect(mockMessagesApi.getGlobalMessageStats).toHaveBeenCalled();
      expect(result.total).toBe(100);
      expect(result.byExecution).toEqual(mockStats.byExecution);
      expect(result.byRole).toEqual(mockStats.byRole);
    });

    it("should handle global stats retrieval failure", async () => {
      const mockError = new Error("Global stats retrieval failed");
      mockMessagesApi.getGlobalMessageStats.mockRejectedValue(mockError);

      await expect(adapter.getGlobalMessageStats()).rejects.toThrow("Global stats retrieval failed");
    });
  });

  describe("normalizeMessages", () => {
    it("should throw error indicating feature is not supported", async () => {
      await expect(adapter.normalizeMessages("agent-loop-123")).rejects.toThrow(
        "normalizeMessages is not supported for workflow executions. This feature is only available for Agent Loops.",
      );
    });

    it("should always throw regardless of input", async () => {
      await expect(adapter.normalizeMessages("any-id")).rejects.toThrow(
        "normalizeMessages is not supported for workflow executions",
      );
    });
  });
});
