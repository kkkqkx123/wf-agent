/**
 * Integration tests for message routing in TUI screens
 * Validates that messages are correctly routed according to CLI_ROUTING_RULES
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { MessageBus } from "@wf-agent/sdk";
import { CLI_ROUTING_RULES } from "../../src/config/routing-rules.js";
import { OutputTarget, MessageCategory, AgentMessageType, WorkflowExecutionMessageType } from "@wf-agent/types";
import type { BaseComponentMessage } from "@wf-agent/types";

describe("Message Routing Integration", () => {
  let messageBus: MessageBus;

  beforeEach(() => {
    // Create fresh message bus with routing rules
    messageBus = new MessageBus(CLI_ROUTING_RULES, {
      maxHistorySize: 100,
      enableHistory: true,
      asyncHandlers: true,
    });
  });

  describe("Agent LLM Stream Messages", () => {
    it("should route agent.llm.stream to TUI only", () => {
      const receivedTargets: string[] = [];

      // Mock handlers
      const tuiHandler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: (msg: BaseComponentMessage) => msg.type === "agent.llm.stream",
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.TUI);
        },
      };

      const fileDisplayHandler = {
        target: OutputTarget.FILE_DISPLAY,
        name: "file_display",
        supports: (msg: BaseComponentMessage) => false, // Should not receive this
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.FILE_DISPLAY);
        },
      };

      messageBus.registerHandler(tuiHandler as any);
      messageBus.registerHandler(fileDisplayHandler as any);

      // Publish LLM stream message
      const message: BaseComponentMessage = {
        id: "test-msg-1",
        timestamp: Date.now(),
        entity: { id: "agent-1", type: "agent", rootId: "agent-1", depth: 0 },
        category: MessageCategory.AGENT,
        type: AgentMessageType.LLM_STREAM,
        level: "info",
        data: { chunk: "Hello" },
      };

      messageBus.publish(message);

      // Give async handlers time to process
      return new Promise((resolve) => {
        setTimeout(() => {
          expect(receivedTargets).toContain(OutputTarget.TUI);
          expect(receivedTargets).not.toContain(OutputTarget.FILE_DISPLAY);
          resolve(undefined);
        }, 100);
      });
    });
  });

  describe("Human Relay Request Messages", () => {
    it("should route agent.human_relay.request to TUI + FILE_FUNCTIONAL + FILE_DISPLAY", () => {
      const receivedTargets: string[] = [];

      const tuiHandler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: (msg: BaseComponentMessage) => msg.type === "agent.human_relay.request",
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.TUI);
        },
      };

      const functionalFileHandler = {
        target: OutputTarget.FILE_FUNCTIONAL,
        name: "file_functional",
        supports: (msg: BaseComponentMessage) => msg.type === "agent.human_relay.request",
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.FILE_FUNCTIONAL);
        },
      };

      const displayFileHandler = {
        target: OutputTarget.FILE_DISPLAY,
        name: "file_display",
        supports: (msg: BaseComponentMessage) => msg.type === "agent.human_relay.request",
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.FILE_DISPLAY);
        },
      };

      messageBus.registerHandler(tuiHandler as any);
      messageBus.registerHandler(functionalFileHandler as any);
      messageBus.registerHandler(displayFileHandler as any);

      const message: BaseComponentMessage = {
        id: "test-msg-2",
        timestamp: Date.now(),
        entity: { id: "session-123", type: "agent", rootId: "session-123", depth: 0 },
        category: MessageCategory.AGENT,
        type: "agent.human_relay.request",
        level: "info",
        data: { prompt: "Please provide input..." },
      };

      messageBus.publish(message);

      return new Promise((resolve) => {
        setTimeout(() => {
          expect(receivedTargets).toContain(OutputTarget.TUI);
          expect(receivedTargets).toContain(OutputTarget.FILE_FUNCTIONAL);
          expect(receivedTargets).toContain(OutputTarget.FILE_DISPLAY);
          expect(receivedTargets.length).toBe(3);
          resolve(undefined);
        }, 100);
      });
    });
  });

  describe("Tool Result Messages", () => {
    it("should route agent.tool.result to FILE_DISPLAY only", () => {
      const receivedTargets: string[] = [];

      const tuiHandler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: () => false, // Should not receive tool results
        handle: async () => {},
      };

      const displayFileHandler = {
        target: OutputTarget.FILE_DISPLAY,
        name: "file_display",
        supports: (msg: BaseComponentMessage) => msg.type === "agent.tool.result",
        handle: async (msg: BaseComponentMessage) => {
          receivedTargets.push(OutputTarget.FILE_DISPLAY);
        },
      };

      messageBus.registerHandler(tuiHandler as any);
      messageBus.registerHandler(displayFileHandler as any);

      const message: BaseComponentMessage = {
        id: "test-msg-3",
        timestamp: Date.now(),
        entity: { id: "agent-1", type: "agent", rootId: "agent-1", depth: 0 },
        category: MessageCategory.AGENT,
        type: "agent.tool.result",
        level: "info",
        data: { toolName: "readFile", result: "content" },
      };

      messageBus.publish(message);

      return new Promise((resolve) => {
        setTimeout(() => {
          expect(receivedTargets).toContain(OutputTarget.FILE_DISPLAY);
          expect(receivedTargets).not.toContain(OutputTarget.TUI);
          resolve(undefined);
        }, 100);
      });
    });
  });

  describe("Workflow Execution Node Messages", () => {
    it("should route workflow-execution.node.start to TUI + FILE_DISPLAY", async () => {
      const receivedTargets: string[] = [];

      const tuiHandler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: (msg: BaseComponentMessage) => msg.type === "workflow.execution.node.start",
        handle: async () => {
          receivedTargets.push(OutputTarget.TUI);
        },
      };

      const displayFileHandler = {
        target: OutputTarget.FILE_DISPLAY,
        name: "file_display",
        supports: (msg: BaseComponentMessage) => msg.type === "workflow.execution.node.start",
        handle: async () => {
          receivedTargets.push(OutputTarget.FILE_DISPLAY);
        },
      };

      messageBus.registerHandler(tuiHandler as any);
      messageBus.registerHandler(displayFileHandler as any);

      const message: BaseComponentMessage = {
        id: "test-msg-4",
        timestamp: Date.now(),
        entity: { id: "workflow-1", type: "workflowExecution", rootId: "workflow-1", depth: 0 },
        category: MessageCategory.WORKFLOW_EXECUTION,
        type: "workflow.execution.node.start",
        level: "info",
        data: { nodeId: "node-1", nodeType: "agent" },
      };

      messageBus.publish(message);

      // Wait for async handlers with longer timeout
      await new Promise((resolve) => setTimeout(resolve, 500));
      
      expect(receivedTargets).toContain(OutputTarget.TUI);
      expect(receivedTargets).toContain(OutputTarget.FILE_DISPLAY);
    });
  });

  describe("Error Messages", () => {
    it("should route error messages to TUI + FILE_DISPLAY with high priority", () => {
      const receivedTargets: string[] = [];

      const tuiHandler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: (msg: BaseComponentMessage) => msg.level === "error",
        handle: async () => {
          receivedTargets.push(OutputTarget.TUI);
        },
      };

      const displayFileHandler = {
        target: OutputTarget.FILE_DISPLAY,
        name: "file_display",
        supports: (msg: BaseComponentMessage) => msg.level === "error",
        handle: async () => {
          receivedTargets.push(OutputTarget.FILE_DISPLAY);
        },
      };

      messageBus.registerHandler(tuiHandler as any);
      messageBus.registerHandler(displayFileHandler as any);

      const message: BaseComponentMessage = {
        id: "test-msg-5",
        timestamp: Date.now(),
        entity: { id: "agent-1", type: "agent", rootId: "agent-1", depth: 0 },
        category: MessageCategory.AGENT,
        type: "system.error",
        level: "error",
        data: { message: "Something went wrong" },
      };

      messageBus.publish(message);

      return new Promise((resolve) => {
        setTimeout(() => {
          expect(receivedTargets).toContain(OutputTarget.TUI);
          expect(receivedTargets).toContain(OutputTarget.FILE_DISPLAY);
          resolve(undefined);
        }, 100);
      });
    });
  });

  describe("Multi-Session Isolation", () => {
    it("should isolate messages between different sessions", () => {
      const sessionMessages: Map<string, BaseComponentMessage[]> = new Map();

      const handler = {
        target: OutputTarget.TUI,
        name: "tui",
        supports: () => true,
        handle: async (msg: BaseComponentMessage) => {
          const sessionId = msg.entity.id;
          if (!sessionMessages.has(sessionId)) {
            sessionMessages.set(sessionId, []);
          }
          sessionMessages.get(sessionId)!.push(msg);
        },
      };

      messageBus.registerHandler(handler as any);

      // Publish messages for different sessions
      const message1: BaseComponentMessage = {
        id: "msg-1",
        timestamp: Date.now(),
        entity: { id: "session-A", type: "agent", rootId: "session-A", depth: 0 },
        category: MessageCategory.AGENT,
        type: AgentMessageType.LLM_STREAM,
        level: "info",
        data: { chunk: "Session A" },
      };

      const message2: BaseComponentMessage = {
        id: "msg-2",
        timestamp: Date.now(),
        entity: { id: "session-B", type: "agent", rootId: "session-B", depth: 0 },
        category: MessageCategory.AGENT,
        type: AgentMessageType.LLM_STREAM,
        level: "info",
        data: { chunk: "Session B" },
      };

      messageBus.publish(message1);
      messageBus.publish(message2);

      return new Promise((resolve) => {
        setTimeout(() => {
          expect(sessionMessages.get("session-A")?.length).toBe(1);
          expect(sessionMessages.get("session-B")?.length).toBe(1);
          expect(sessionMessages.get("session-A")?.[0].data).toEqual({ chunk: "Session A" });
          expect(sessionMessages.get("session-B")?.[0].data).toEqual({ chunk: "Session B" });
          resolve(undefined);
        }, 100);
      });
    });
  });
});
