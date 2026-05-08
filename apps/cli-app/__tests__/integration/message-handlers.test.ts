/**
 * Tests for TUI Message Handlers
 * Validates that handlers correctly process and route messages
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TUIHandler } from "../../src/messaging/handlers/tui-handler.js";
import { FunctionalFileHandler } from "../../src/messaging/handlers/functional-file-handler.js";
import { DisplayFileHandler } from "../../src/messaging/handlers/display-file-handler.js";
import { OutputTarget, MessageCategory } from "@wf-agent/types";
import type { BaseComponentMessage } from "@wf-agent/types";
import type { TUI } from "../../src/tui/core/tui.js";
import type { FileIOService } from "../../src/io/file-io-service.js";

// Helper function to create test messages with proper entity structure
function createTestMessage(overrides: Partial<BaseComponentMessage>): BaseComponentMessage {
  return {
    id: overrides.id || "test-msg",
    timestamp: overrides.timestamp || Date.now(),
    entity: overrides.entity || { id: "test-entity", type: "agent", rootId: "test-entity", depth: 0 },
    category: overrides.category || MessageCategory.AGENT,
    type: overrides.type || "agent.test",
    level: overrides.level || "info",
    data: overrides.data || {},
  };
}

describe("TUI Handler", () => {
  let handler: TUIHandler;
  let mockTUI: any;

  beforeEach(() => {
    mockTUI = {
      render: vi.fn(),
      requestRender: vi.fn(),
    };
    handler = new TUIHandler(mockTUI as unknown as TUI);
  });

  describe("target and name", () => {
    it("should have correct target", () => {
      expect(handler.target).toBe(OutputTarget.TUI);
    });

    it("should have correct name", () => {
      expect(handler.name).toBe("tui");
    });
  });

  describe("supports", () => {
    it("should support agent.llm.stream messages", () => {
      const message = createTestMessage({
        id: "test-1",
        type: "agent.llm.stream",
        data: { chunk: "test" },
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support agent.tool.call_start messages", () => {
      const message = createTestMessage({
        id: "test-2",
        type: "agent.tool.call_start",
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support agent.tool.call_end messages", () => {
      const message = createTestMessage({
        id: "test-3",
        type: "agent.tool.call_end",
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support agent.human_relay.request messages", () => {
      const message = createTestMessage({
        id: "test-4",
        type: "agent.human_relay.request",
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should NOT support tool.result messages (goes to file only)", () => {
      const message = createTestMessage({
        id: "test-5",
        type: "agent.tool.result",
      });

      expect(handler.supports(message)).toBe(false);
    });

    it("should NOT support checkpoint.create messages (goes to file only)", () => {
      const message = createTestMessage({
        id: "test-6",
        type: "workflow-execution.checkpoint.create",
        entity: { id: "workflow-1", type: "workflowExecution", rootId: "workflow-1", depth: 0 },
        category: MessageCategory.WORKFLOW_EXECUTION,
      });

      expect(handler.supports(message)).toBe(false);
    });
  });

  describe("handle", () => {
    it("should handle supported messages without throwing", async () => {
      const message = createTestMessage({
        id: "test-7",
        type: "agent.llm.stream",
        data: { chunk: "Hello" },
      });

      await expect(handler.handle(message)).resolves.not.toThrow();
    });
  });
});

describe("Functional File Handler", () => {
  let handler: FunctionalFileHandler;
  let mockFileIO: any;

  beforeEach(() => {
    mockFileIO = {
      writeHumanRelayOutput: vi.fn(),
      getSessionPaths: vi.fn(),
    };
    handler = new FunctionalFileHandler(mockFileIO as unknown as FileIOService);
  });

  describe("target and name", () => {
    it("should have correct target", () => {
      expect(handler.target).toBe(OutputTarget.FILE_FUNCTIONAL);
    });

    it("should have correct name", () => {
      expect(handler.name).toBe("file_functional");
    });
  });

  describe("supports", () => {
    it("should ONLY support agent.human_relay.request messages", () => {
      const humanRelayMessage = createTestMessage({
        id: "test-1",
        type: "agent.human_relay.request",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
        data: { prompt: "Test prompt" },
      });

      expect(handler.supports(humanRelayMessage)).toBe(true);
    });

    it("should NOT support other message types", () => {
      const otherMessage = createTestMessage({
        id: "test-2",
        type: "agent.llm.stream",
      });

      expect(handler.supports(otherMessage)).toBe(false);
    });
  });

  describe("handle", () => {
    it("should write human relay output to file", async () => {
      const message = createTestMessage({
        id: "test-3",
        type: "agent.human_relay.request",
        entity: { id: "session-123", type: "agent", rootId: "session-123", depth: 0 },
        data: { prompt: "Please respond..." },
      });

      await handler.handle(message);

      expect(mockFileIO.writeHumanRelayOutput).toHaveBeenCalledWith({
        sessionId: "session-123",
        content: "Please respond...",
      });
    });

    it("should generate session ID if not provided", async () => {
      const message = createTestMessage({
        id: "test-4",
        type: "agent.human_relay.request",
        entity: { id: "", type: "agent", rootId: "", depth: 0 }, // Empty ID
        data: { prompt: "Test" },
      });

      await handler.handle(message);

      expect(mockFileIO.writeHumanRelayOutput).toHaveBeenCalled();
      const callArgs = mockFileIO.writeHumanRelayOutput.mock.calls[0][0];
      expect(callArgs.sessionId).toMatch(/^session-\d+$/);
    });
  });
});

describe("Display File Handler", () => {
  let handler: DisplayFileHandler;
  let mockFileIO: any;

  beforeEach(() => {
    mockFileIO = {
      updateDisplayOutput: vi.fn(),
    };
    handler = new DisplayFileHandler(mockFileIO as unknown as FileIOService);
  });

  afterEach(async () => {
    // Clean up any pending timers
    await handler.flush();
  });

  describe("target and name", () => {
    it("should have correct target", () => {
      expect(handler.target).toBe(OutputTarget.FILE_DISPLAY);
    });

    it("should have correct name", () => {
      expect(handler.name).toBe("file_display");
    });
  });

  describe("supports", () => {
    it("should support agent.tool.result messages", () => {
      const message = createTestMessage({
        id: "test-1",
        type: "agent.tool.result",
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support workflow-execution.node.start messages", () => {
      const message = createTestMessage({
        id: "test-2",
        type: "workflow-execution.node.start",
        entity: { id: "workflow-1", type: "workflowExecution", rootId: "workflow-1", depth: 0 },
        category: MessageCategory.WORKFLOW_EXECUTION,
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support workflow-execution.node.end messages", () => {
      const message = createTestMessage({
        id: "test-3",
        type: "workflow-execution.node.end",
        entity: { id: "workflow-1", type: "workflowExecution", rootId: "workflow-1", depth: 0 },
        category: MessageCategory.WORKFLOW_EXECUTION,
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support workflow-execution.checkpoint.create messages", () => {
      const message = createTestMessage({
        id: "test-4",
        type: "workflow-execution.checkpoint.create",
        entity: { id: "workflow-1", type: "workflowExecution", rootId: "workflow-1", depth: 0 },
        category: MessageCategory.WORKFLOW_EXECUTION,
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should support agent.iteration.start messages", () => {
      const message = createTestMessage({
        id: "test-5",
        type: "agent.iteration.start",
      });

      expect(handler.supports(message)).toBe(true);
    });

    it("should NOT support agent.llm.stream messages", () => {
      const message = createTestMessage({
        id: "test-6",
        type: "agent.llm.stream",
      });

      expect(handler.supports(message)).toBe(false);
    });
  });

  describe("handle", () => {
    it("should buffer messages and schedule flush", async () => {
      const message = createTestMessage({
        id: "test-7",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
        data: { toolName: "readFile" },
      });

      await handler.handle(message);

      // Should not flush immediately
      expect(mockFileIO.updateDisplayOutput).not.toHaveBeenCalled();
    });

    it("should flush buffered messages after interval", async () => {
      vi.useFakeTimers();

      const message = createTestMessage({
        id: "test-8",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
        data: { toolName: "readFile" },
      });

      await handler.handle(message);

      // Advance timer by flush interval
      vi.advanceTimersByTime(2000);

      expect(mockFileIO.updateDisplayOutput).toHaveBeenCalled();

      vi.useRealTimers();
    });

    it("should create proper sections for tool results", async () => {
      const message = createTestMessage({
        id: "test-9",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
        data: { toolName: "writeFile" },
      });

      await handler.handle(message);
      await handler.flush();

      expect(mockFileIO.updateDisplayOutput).toHaveBeenCalledWith(
        expect.objectContaining({
          sessionId: "session-1",
          sections: expect.arrayContaining([
            expect.objectContaining({
              title: expect.stringContaining("writeFile"),
            }),
          ]),
        })
      );
    });
  });

  describe("flush", () => {
    it("should flush all buffered messages immediately", async () => {
      const message = createTestMessage({
        id: "test-10",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
      });

      await handler.handle(message);
      await handler.flush();

      expect(mockFileIO.updateDisplayOutput).toHaveBeenCalled();
    });

    it("should clear buffer after flush", async () => {
      const message = createTestMessage({
        id: "test-11",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
      });

      await handler.handle(message);
      await handler.flush();

      // Reset mock to check for additional calls
      mockFileIO.updateDisplayOutput.mockClear();

      // Flush again - should not write again since buffer is cleared
      await handler.flush();

      expect(mockFileIO.updateDisplayOutput).not.toHaveBeenCalled();
    });
  });

  describe("close", () => {
    it("should flush remaining buffers on close", async () => {
      const message = createTestMessage({
        id: "test-12",
        type: "agent.tool.result",
        entity: { id: "session-1", type: "agent", rootId: "session-1", depth: 0 },
      });

      await handler.handle(message);
      await handler.close();

      expect(mockFileIO.updateDisplayOutput).toHaveBeenCalled();
    });
  });
});
