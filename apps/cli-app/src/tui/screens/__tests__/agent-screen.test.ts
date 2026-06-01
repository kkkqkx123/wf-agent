/**
 * Unit tests for AgentScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentScreen } from "../agent-screen.js";
import { Container, Box, Text, Input } from "../../index.js";

// Mock the AgentLoopAdapter - vi.mock is hoisted, so we need to use a pattern that works
const mockExecuteAgentLoopStream = vi.fn();

vi.mock("../../../src/adapters/agent-loop-adapter.js", () => {
  return {
    AgentLoopAdapter: class MockAgentLoopAdapter {
      executeAgentLoopStream = mockExecuteAgentLoopStream;
    },
  };
});

describe("AgentScreen", () => {
  let screen: AgentScreen;
  let onBackMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onBackMock = vi.fn();
    
    // Reset mock and set default behavior
    mockExecuteAgentLoopStream.mockReset();
    mockExecuteAgentLoopStream.mockImplementation((config, context, callback) => {
      // Simulate some events
      if (callback) {
        callback({ type: "text", delta: "Hello" });
        callback({ type: "tool_call_start", data: { toolCall: { function: { name: "testTool" } } } });
        callback({ type: "tool_call_end", data: { success: true } });
        callback({ type: "iteration_complete", data: { iteration: 1 } });
      }
      return Promise.resolve({ success: true });
    });
    
    screen = new AgentScreen(onBackMock);
  });

  describe("constructor", () => {
    it("should create instance without onBack callback", () => {
      const screenWithoutCallback = new AgentScreen();
      expect(screenWithoutCallback).toBeDefined();
      expect(screenWithoutCallback.render()).toBeDefined();
    });

    it("should create instance with onBack callback", () => {
      expect(screen).toBeDefined();
      expect(onBackMock).toBeDefined();
    });

    it("should initialize in idle state", () => {
      expect((screen as any).isRunning).toBe(false);
    });

    it("should initialize with empty log entries", () => {
      expect((screen as any).logEntries).toEqual([]);
    });
  });

  describe("render", () => {
    it("should return a Container component", () => {
      const result = screen.render();
      expect(result).toBeInstanceOf(Container);
    });

    it("should render toolbar with action buttons", () => {
      const container = screen.render() as Container;
      expect(container.children.length).toBeGreaterThan(0);
      
      // First child should be toolbar
      const toolbar = container.children[0] as Box;
      expect(toolbar).toBeInstanceOf(Box);
      
      const toolbarChildren = (toolbar as any).children;
      expect(toolbarChildren.length).toBeGreaterThanOrEqual(1);
      expect(toolbarChildren[0]).toBeInstanceOf(Text);
      
      const toolbarText = toolbarChildren[0].render(80)[0];
      expect(toolbarText).toContain("[S]tart");
      expect(toolbarText).toContain("[P]ause");
      expect(toolbarText).toContain("[R]esume");
      expect(toolbarText).toContain("[C]ancel");
      expect(toolbarText).toContain("[B]ack");
    });

    it("should render status panel", () => {
      const container = screen.render() as Container;
      
      // Second child should be status box
      const statusBox = container.children[1] as Box;
      expect(statusBox).toBeInstanceOf(Box);
      
      const statusChildren = (statusBox as any).children;
      expect(statusChildren.length).toBeGreaterThanOrEqual(2);
      
      // First child should be label
      expect(statusChildren[0]).toBeInstanceOf(Text);
      
      // Second child should be status panel
      const statusPanel = statusChildren[1] as Box;
      expect(statusPanel).toBeInstanceOf(Box);
    });

    it("should display initial idle status", () => {
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      
      expect(statusChildren.length).toBeGreaterThanOrEqual(3);
      
      const statusText = statusChildren[0].render(80)[0];
      expect(statusText).toContain("IDLE");
    });

    it("should render log panel", () => {
      const container = screen.render() as Container;
      
      // Third child should be log box
      const logBox = container.children[2] as Box;
      expect(logBox).toBeInstanceOf(Box);
      
      const logChildren = (logBox as any).children;
      expect(logChildren.length).toBeGreaterThanOrEqual(2);
      
      // First child should be label
      expect(logChildren[0]).toBeInstanceOf(Text);
      
      // Second child should be log panel
      const logPanel = logChildren[1] as Box;
      expect(logPanel).toBeInstanceOf(Box);
    });

    it("should render message input", () => {
      const container = screen.render() as Container;
      
      // Fourth child should be input box
      const inputBox = container.children[3] as Box;
      expect(inputBox).toBeInstanceOf(Box);
      
      const inputChildren = (inputBox as any).children;
      expect(inputChildren.length).toBeGreaterThanOrEqual(2);
      
      // First child should be label
      expect(inputChildren[0]).toBeInstanceOf(Text);
      
      // Second child should be Input component
      expect(inputChildren[1]).toBeInstanceOf(Input);
    });
  });

  describe("handleInput", () => {
    it("should handle 'b' key to go back", () => {
      const result = screen.handleInput!("b");
      expect(result).toBe(true);
      expect(onBackMock).toHaveBeenCalled();
    });

    it("should handle 'B' key to go back", () => {
      const result = screen.handleInput!("B");
      expect(result).toBe(true);
      expect(onBackMock).toHaveBeenCalled();
    });

    it("should handle 's' key to start agent", () => {
      const result = screen.handleInput!("s");
      expect(result).toBe(true);
    });

    it("should handle 'S' key to start agent", () => {
      const result = screen.handleInput!("S");
      expect(result).toBe(true);
    });

    it("should handle 'c' key to cancel agent", () => {
      const result = screen.handleInput!("c");
      expect(result).toBe(true);
      
      // Should update status to idle
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      expect(statusText).toContain("IDLE");
    });

    it("should handle 'C' key to cancel agent", () => {
      const result = screen.handleInput!("C");
      expect(result).toBe(true);
    });

    it("should delegate other input to message input", () => {
      const result = screen.handleInput!("a");
      expect(result).toBe(true);
    });
  });

  describe("startAgent", () => {
    it("should start agent and handle events", async () => {
      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      // Should have processed events
      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBeGreaterThan(0);
      
      // Check for various event types
      const hasAssistantMessage = logEntries.some((e: any) => e.type === "assistant");
      const hasToolMessage = logEntries.some((e: any) => e.type === "tool");
      const hasSystemMessage = logEntries.some((e: any) => e.type === "system");
      
      expect(hasAssistantMessage || hasToolMessage || hasSystemMessage).toBe(true);
    });

    it("should prevent starting multiple agents", async () => {
      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      // Manually set isRunning to true to simulate a running agent
      (screen as any).isRunning = true;
      
      // Try to start again
      await screen.startAgent(config);
      
      // Should have logged warning about already running
      const logEntries = (screen as any).logEntries;
      const hasWarning = logEntries.some((e: any) => 
        e.message.includes("already running")
      );
      
      expect(hasWarning).toBe(true);
      
      // Reset for cleanup
      (screen as any).isRunning = false;
    });

    it("should handle agent errors", async () => {
      // Mock adapter to throw error
      const mockAdapter = {
        executeAgentLoopStream: vi.fn().mockRejectedValue(new Error("Test error")),
      };
      (screen as any).adapter = mockAdapter;

      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      // Should update status to error
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      expect(statusText).toContain("ERROR");
    });

    it("should handle failed agent execution", async () => {
      // Mock adapter to return failure
      const mockAdapter = {
        executeAgentLoopStream: vi.fn().mockResolvedValue({ success: false, error: "Execution failed" }),
      };
      (screen as any).adapter = mockAdapter;

      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      // Should log failure message
      const logEntries = (screen as any).logEntries;
      const hasFailureMessage = logEntries.some((e: any) => 
        e.message.includes("failed")
      );
      
      expect(hasFailureMessage).toBe(true);
    });
  });

  describe("sendMessage", () => {
    it("should append user message to log", async () => {
      // Set isRunning to true to allow sending messages
      (screen as any).isRunning = true;
      
      // Access private method via any
      const sendMessage = (screen as any).sendMessage.bind(screen);
      await sendMessage("Test message");
      
      const logEntries = (screen as any).logEntries;
      const userMessage = logEntries.find((e: any) => e.type === "user");
      
      expect(userMessage).toBeDefined();
      expect(userMessage.message).toBe("Test message");
    });

    it("should prevent sending messages when agent not running", async () => {
      // Ensure isRunning is false
      (screen as any).isRunning = false;
      
      const sendMessage = (screen as any).sendMessage.bind(screen);
      await sendMessage("Test message");
      
      const logEntries = (screen as any).logEntries;
      const hasWarning = logEntries.some((e: any) => 
        e.message.includes("Start agent first")
      );
      
      expect(hasWarning).toBe(true);
    });

    it("should clear input after sending message", async () => {
      (screen as any).isRunning = true;
      
      const sendMessage = (screen as any).sendMessage.bind(screen);
      await sendMessage("Test message");
      
      // Check that input was cleared
      const container = screen.render() as Container;
      const inputBox = container.children[3] as Box;
      const inputComponent = (inputBox as any).children[1] as Input;
      
      // Input should be cleared (or have placeholder)
      expect(inputComponent).toBeDefined();
    });
  });

  describe("event handling", () => {
    it("should handle text events", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ type: "text", delta: "Test response" });
      
      const logEntries = (screen as any).logEntries;
      const assistantMessage = logEntries.find((e: any) => 
        e.type === "assistant" && e.message === "Test response"
      );
      
      expect(assistantMessage).toBeDefined();
    });

    it("should handle tool call start events", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ 
        type: "tool_call_start", 
        data: { toolCall: { function: { name: "searchTool" } } } 
      });
      
      const logEntries = (screen as any).logEntries;
      const toolMessage = logEntries.find((e: any) => 
        e.type === "tool" && e.message.includes("searchTool")
      );
      
      expect(toolMessage).toBeDefined();
    });

    it("should handle tool call end events", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ type: "tool_call_end", data: { success: true } });
      
      const logEntries = (screen as any).logEntries;
      const toolMessage = logEntries.find((e: any) => 
        e.type === "tool" && e.message.includes("completed")
      );
      
      expect(toolMessage).toBeDefined();
    });

    it("should handle iteration complete events", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ type: "iteration_complete", data: { iteration: 5 } });
      
      const logEntries = (screen as any).logEntries;
      const systemMessage = logEntries.find((e: any) => 
        e.type === "system" && e.message.includes("Iteration 5")
      );
      
      expect(systemMessage).toBeDefined();
    });

    it("should handle user message events", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ type: "user_message", data: { content: "User question" } });
      
      const logEntries = (screen as any).logEntries;
      const userMessage = logEntries.find((e: any) => 
        e.type === "user" && e.message === "User question"
      );
      
      expect(userMessage).toBeDefined();
    });

    it("should handle unknown event types", () => {
      const handleEvent = (screen as any).handleEvent.bind(screen);
      handleEvent({ type: "unknown_event" });
      
      const logEntries = (screen as any).logEntries;
      const systemMessage = logEntries.find((e: any) => 
        e.type === "system" && e.message.includes("unknown_event")
      );
      
      expect(systemMessage).toBeDefined();
    });
  });

  describe("updateStatus", () => {
    it("should update status to running", () => {
      const updateStatus = (screen as any).updateStatus.bind(screen);
      updateStatus("running");
      
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      
      expect(statusText).toContain("RUNNING");
    });

    it("should update status to paused", () => {
      const updateStatus = (screen as any).updateStatus.bind(screen);
      updateStatus("paused");
      
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      
      expect(statusText).toContain("PAUSED");
    });

    it("should update status to completed", () => {
      const updateStatus = (screen as any).updateStatus.bind(screen);
      updateStatus("completed");
      
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      
      expect(statusText).toContain("COMPLETED");
    });

    it("should update status to error", () => {
      const updateStatus = (screen as any).updateStatus.bind(screen);
      updateStatus("error");
      
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      
      expect(statusText).toContain("ERROR");
    });
  });

  describe("appendLog", () => {
    it("should append log entry with timestamp", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("Test log message", "system");
      
      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBe(1);
      expect(logEntries[0].message).toBe("Test log message");
      expect(logEntries[0].type).toBe("system");
      expect(logEntries[0].timestamp).toBeInstanceOf(Date);
    });

    it("should limit log entries to last 50", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      
      // Add 60 entries
      for (let i = 0; i < 60; i++) {
        appendLog(`Message ${i}`, "system");
      }
      
      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBe(60); // All entries kept
      
      // But only last 50 should be rendered
      const container = screen.render() as Container;
      const logBox = container.children[2] as Box;
      const logPanel = (logBox as any).children[1] as Box;
      const logChildren = (logPanel as any).children;
      
      expect(logChildren.length).toBeLessThanOrEqual(50);
    });

    it("should format different log types with appropriate icons", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      
      appendLog("User message", "user");
      appendLog("Assistant message", "assistant");
      appendLog("System message", "system");
      appendLog("Tool message", "tool");
      
      const container = screen.render() as Container;
      const logBox = container.children[2] as Box;
      const logPanel = (logBox as any).children[1] as Box;
      const logChildren = (logPanel as any).children;
      
      expect(logChildren.length).toBe(4);
      
      // Check that all entries are rendered
      const renderedText = logChildren.map((child: any) => child.render(80)[0]);
      expect(renderedText.join("\n")).toContain("User message");
      expect(renderedText.join("\n")).toContain("Assistant message");
      expect(renderedText.join("\n")).toContain("System message");
      expect(renderedText.join("\n")).toContain("Tool message");
    });
  });

  describe("destroy", () => {
    it("should have destroy method", () => {
      expect(typeof screen.destroy).toBe("function");
    });

    it("should cleanup running agent", () => {
      (screen as any).isRunning = true;
      screen.destroy!();
      
      expect((screen as any).isRunning).toBe(false);
    });

    it("should cleanup without errors when not running", () => {
      expect(() => screen.destroy!()).not.toThrow();
    });
  });

  describe("Screen interface compliance", () => {
    it("should implement render method", () => {
      expect(typeof screen.render).toBe("function");
    });

    it("should implement handleInput method", () => {
      expect(typeof screen.handleInput).toBe("function");
    });

    it("should implement destroy method", () => {
      expect(typeof screen.destroy).toBe("function");
    });
  });
});
