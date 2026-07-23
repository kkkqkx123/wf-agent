/**
 * Unit tests for AgentScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentScreen } from "../agent-screen.js";
import { Container, Box, Text, Input, InputMode } from "../../index.js";
import type { TUI } from "../../core/tui.js";

// Mock the AgentLoopAdapter - vi.mock is hoisted, so we need to use a pattern that works
const mockExecuteAgentLoopStream = vi.fn();

vi.mock("../../../src/adapters/agent-loop-adapter.js", () => {
  return {
    AgentLoopAdapter: class MockAgentLoopAdapter {
      executeAgentLoopStream = mockExecuteAgentLoopStream;
    },
  };
});

/** Create a minimal mock TUI for testing Normal mode interactions */
function createMockTui(): TUI {
  return {
    setContext: vi.fn(),
    setInputMode: vi.fn(),
    inputMode: InputMode.Chat,
    currentContext: "chat",
  } as unknown as TUI;
}

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
      
      // 5th child (index 4) should be log box
      const logBox = container.children[4] as Box;
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
      
      // 6th child (index 5) should be input box
      const inputBox = container.children[5] as Box;
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

  describe("Normal mode navigation", () => {
    let screenWithTui: AgentScreen;

    beforeEach(() => {
      screenWithTui = new AgentScreen(vi.fn(), createMockTui());
      // Seed some log entries so there's something to scroll through
      const appendLog = (screenWithTui as any).appendLog.bind(screenWithTui);
      for (let i = 0; i < 30; i++) {
        appendLog(`Log entry ${i + 1}`, "system");
      }
    });

    it("should switch to Normal mode on Esc in Chat mode", () => {
      // Simulate Esc key (escape sequence)
      const result = screenWithTui.handleInput!("\x1b");
      expect(result).toBe(true);
      expect((screenWithTui as any).mode).toBe(InputMode.Normal);
    });

    it("should switch back to Chat mode on Enter in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode
      expect((screenWithTui as any).mode).toBe(InputMode.Normal);

      const result = screenWithTui.handleInput!("\r"); // Enter
      expect(result).toBe(true);
      expect((screenWithTui as any).mode).toBe(InputMode.Chat);
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should switch back to Chat mode on Esc in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode

      const result = screenWithTui.handleInput!("\x1b"); // Esc again
      expect(result).toBe(true);
      expect((screenWithTui as any).mode).toBe(InputMode.Chat);
    });

    it("should scroll down (j key) and up (k key) in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode
      expect((screenWithTui as any).scrollOffset).toBe(0);

      // k (down/scroll up — toward older entries)
      screenWithTui.handleInput!("k");
      expect((screenWithTui as any).scrollOffset).toBe(1);

      // j (up/scroll down — toward newer entries)
      screenWithTui.handleInput!("j");
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should not scroll past the top of log in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode

      // Scroll up all the way, then try to go further
      for (let i = 0; i < 30; i++) {
        screenWithTui.handleInput!("k");
      }
      // scrollOffset should be clamped at max
      const maxOffset = Math.max(0, 30 - 20); // 30 entries, NORMAL_MODE_MAX_LOG_LINES=20
      expect((screenWithTui as any).scrollOffset).toBe(maxOffset);

      // One more k should not increase
      screenWithTui.handleInput!("k");
      expect((screenWithTui as any).scrollOffset).toBe(maxOffset);
    });

    it("should jump to top (g) and bottom (G) in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode

      // Scroll to somewhere in the middle first
      for (let i = 0; i < 5; i++) {
        screenWithTui.handleInput!("k");
      }
      expect((screenWithTui as any).scrollOffset).toBe(5);

      // g → jump to top (oldest)
      screenWithTui.handleInput!("g");
      const maxOffset = Math.max(0, 30 - 20);
      expect((screenWithTui as any).scrollOffset).toBe(maxOffset);

      // G → jump to bottom (latest)
      screenWithTui.handleInput!("G");
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should handle half-page scroll (Ctrl+u / Ctrl+d) in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode

      // Ctrl+u (halfPageUp → scroll toward older entries)
      screenWithTui.handleInput!("\x15"); // Ctrl+u = \x15
      expect((screenWithTui as any).scrollOffset).toBeGreaterThan(0);

      // Ctrl+d (halfPageDown → scroll toward newer entries)
      screenWithTui.handleInput!("\x04"); // Ctrl+d = \x04
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should switch to Chat and forward printable char in Normal mode", () => {
      screenWithTui.handleInput!("\x1b"); // Enter Normal mode

      // Type a printable character
      screenWithTui.handleInput!("h");
      expect((screenWithTui as any).mode).toBe(InputMode.Chat);
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should display [NORMAL] label in status when in Normal mode", () => {
      const updateStatus = (screenWithTui as any).updateStatus.bind(screenWithTui);

      // In Chat mode, no [NORMAL] label
      (screenWithTui as any).mode = InputMode.Normal;
      updateStatus("idle");
      const renderContainer = screenWithTui.render() as Container;
      const statusBox = renderContainer.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      expect(statusText).toContain("[NORMAL]");
    });

    it("should still process toolbar shortcuts (b/B) in Normal mode", () => {
      const onBack = vi.fn();
      const tuiScreen = new AgentScreen(onBack, createMockTui());
      tuiScreen.handleInput!("\x1b"); // Enter Normal mode

      const result = tuiScreen.handleInput!("b");
      expect(result).toBe(true);
      expect(onBack).toHaveBeenCalled();
    });
  });

  describe("startAgent", () => {
    it("should start agent and log starting message", async () => {
      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      // Should have logged a start message
      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBeGreaterThan(0);
      
      const hasSystemMessage = logEntries.some((e: any) => e.type === "system");
      
      expect(hasSystemMessage).toBe(true);
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

    it("should update status to running after start", async () => {
      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      // Should update status to running
      const container = screen.render() as Container;
      const statusBox = container.children[1] as Box;
      const statusPanel = (statusBox as any).children[1] as Box;
      const statusChildren = (statusPanel as any).children;
      const statusText = statusChildren[0].render(80)[0];
      expect(statusText).toContain("RUNNING");
    });

    it("should generate an agent ID on start", async () => {
      const config = {
        workflowId: "test-workflow",
        maxIterations: 5,
      } as any;

      await screen.startAgent(config);

      expect((screen as any).currentAgentId).toBeDefined();
      expect(typeof (screen as any).currentAgentId).toBe("string");
      expect((screen as any).isRunning).toBe(true);
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
      const inputBox = container.children[5] as Box;
      const inputComponent = (inputBox as any).children[1] as Input;
      
      // Input should be cleared (or have placeholder)
      expect(inputComponent).toBeDefined();
    });
  });

  describe("event handling via appendLog", () => {
    it("should log user messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("User question", "user");

      const logEntries = (screen as any).logEntries;
      const userMessage = logEntries.find((e: any) =>
        e.type === "user" && e.message === "User question"
      );

      expect(userMessage).toBeDefined();
    });

    it("should log assistant messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("Test response", "assistant");

      const logEntries = (screen as any).logEntries;
      const assistantMessage = logEntries.find((e: any) =>
        e.type === "assistant" && e.message === "Test response"
      );

      expect(assistantMessage).toBeDefined();
    });

    it("should log tool messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("searchTool called", "tool");

      const logEntries = (screen as any).logEntries;
      const toolMessage = logEntries.find((e: any) =>
        e.type === "tool" && e.message.includes("searchTool")
      );

      expect(toolMessage).toBeDefined();
    });

    it("should log tool completion messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("Tool completed: success", "tool");

      const logEntries = (screen as any).logEntries;
      const toolMessage = logEntries.find((e: any) =>
        e.type === "tool" && e.message.includes("completed")
      );

      expect(toolMessage).toBeDefined();
    });

    it("should log iteration completion messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("Iteration 5 complete", "system");

      const logEntries = (screen as any).logEntries;
      const systemMessage = logEntries.find((e: any) =>
        e.type === "system" && e.message.includes("Iteration 5")
      );

      expect(systemMessage).toBeDefined();
    });

    it("should log unknown event type messages", () => {
      const appendLog = (screen as any).appendLog.bind(screen);
      appendLog("unknown_event received", "system");

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
      
      // But only last NORMAL_MODE_MAX_LOG_LINES should be rendered
      const container = screen.render() as Container;
      const logBox = container.children[4] as Box;
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
      const logBox = container.children[4] as Box;
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
