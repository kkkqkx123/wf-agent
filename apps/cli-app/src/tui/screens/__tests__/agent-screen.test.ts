/**
 * Unit tests for AgentScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentScreen } from "../agent-screen.js";
import { Container, Text, Input, InteractionPhase } from "../../index.js";
import type { TUI } from "../../core/tui.js";

const mockExecuteAgentLoopStream = vi.fn();

vi.mock("../../../src/adapters/agent-loop-adapter.js", () => {
  return {
    AgentLoopAdapter: class MockAgentLoopAdapter {
      executeAgentLoopStream = mockExecuteAgentLoopStream;
    },
  };
});

function createMockTui(): TUI {
  const phaseState = { current: InteractionPhase.Idle };
  return {
    setContext: vi.fn(),
    setInputMode: vi.fn(),
    requestRender: vi.fn(),
    inputMode: InteractionPhase.Idle as any,
    currentContext: "chat",
    phaseManager: {
      get phase() { return phaseState.current; },
      transition: vi.fn((to: InteractionPhase) => { phaseState.current = to; return true; }),
      handleInput: vi.fn(() => ({ consumed: false, buffered: false })),
      clearBufferedInput: vi.fn(),
    },
    get phase() { return phaseState.current; },
  } as unknown as TUI;
}

describe("AgentScreen", () => {
  let screen: AgentScreen;
  let onBackMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onBackMock = vi.fn();

    mockExecuteAgentLoopStream.mockReset();
    mockExecuteAgentLoopStream.mockImplementation((_config, _context, callback) => {
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

    it("should render status line as first child", () => {
      const container = screen.render() as Container;
      const statusLine = container.children[0] as Text;
      expect(statusLine).toBeInstanceOf(Text);
      const rendered = statusLine.render(80);
      expect(rendered[0]).toContain("IDLE");
    });

    it("should render message input as last child", () => {
      const container = screen.render() as Container;
      const input = container.children[container.children.length - 1] as Input;
      expect(input).toBeInstanceOf(Input);
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

    it("should delegate other input to message input", () => {
      const result = screen.handleInput!("a");
      expect(result).toBe(true);
    });
  });

  describe("Normal mode navigation", () => {
    let screenWithTui: AgentScreen;

    beforeEach(() => {
      screenWithTui = new AgentScreen(vi.fn(), createMockTui());
      const appendLog = (screenWithTui as any).appendLog.bind(screenWithTui);
      for (let i = 0; i < 30; i++) {
        appendLog(`Log entry ${i + 1}`, "system");
      }
    });

    it("should switch to Normal mode on Esc in Chat mode", () => {
      const result = screenWithTui.handleInput!("\x1b");
      expect(result).toBe(true);
      expect(screenWithTui["tui"]["phaseManager"]["phase"]).toBe(InteractionPhase.Normal);
    });

    it("should switch back to Chat mode on Enter in Normal mode", () => {
      screenWithTui.handleInput!("\x1b");
      expect(screenWithTui["tui"]["phaseManager"]["phase"]).toBe(InteractionPhase.Normal);

      const result = screenWithTui.handleInput!("\r");
      expect(result).toBe(true);
      expect(screenWithTui["tui"]["phaseManager"]["phase"]).toBe(InteractionPhase.Idle);
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should scroll down (j key) and up (k key) in Normal mode", () => {
      screenWithTui.handleInput!("\x1b");
      expect((screenWithTui as any).scrollOffset).toBe(0);

      screenWithTui.handleInput!("k");
      expect((screenWithTui as any).scrollOffset).toBe(1);

      screenWithTui.handleInput!("j");
      expect((screenWithTui as any).scrollOffset).toBe(0);
    });

    it("should not scroll past the top of log in Normal mode", () => {
      screenWithTui.handleInput!("\x1b");

      for (let i = 0; i < 30; i++) {
        screenWithTui.handleInput!("k");
      }
      const maxOffset = Math.max(0, 30 - 20);
      expect((screenWithTui as any).scrollOffset).toBe(maxOffset);

      screenWithTui.handleInput!("k");
      expect((screenWithTui as any).scrollOffset).toBe(maxOffset);
    });

    it("should still process toolbar shortcuts (b/B) in Normal mode", () => {
      const onBack = vi.fn();
      const tuiScreen = new AgentScreen(onBack, createMockTui());
      tuiScreen.handleInput!("\x1b");

      const result = tuiScreen.handleInput!("b");
      expect(result).toBe(true);
      expect(onBack).toHaveBeenCalled();
    });
  });

  describe("Streaming phase", () => {
    let screenWithTui: AgentScreen;
    let tui: TUI;

    beforeEach(() => {
      tui = createMockTui();
      screenWithTui = new AgentScreen(vi.fn(), tui);
      tui.phaseManager.transition(InteractionPhase.Streaming);
      vi.clearAllMocks();
    });

    it("should cancel turn on Ctrl+C during Streaming", () => {
      const result = screenWithTui.handleInput!("\x03");
      expect(result).toBe(true);
      expect(tui.phaseManager.transition).toHaveBeenCalledWith(InteractionPhase.Idle);
      expect(tui.phaseManager.clearBufferedInput).toHaveBeenCalled();
    });
  });

  describe("Approval phase", () => {
    let screenWithTui: AgentScreen;
    let tui: TUI;

    beforeEach(() => {
      tui = createMockTui();
      screenWithTui = new AgentScreen(vi.fn(), tui);
      tui.phaseManager.transition(InteractionPhase.Approval);
      vi.clearAllMocks();
    });

    it("should approve on y/Y key", () => {
      const result = screenWithTui.handleInput!("y");
      expect(result).toBe(true);
      expect(tui.phaseManager.transition).toHaveBeenCalledWith(InteractionPhase.Streaming);
    });

    it("should reject on n/N key", () => {
      const result = screenWithTui.handleInput!("n");
      expect(result).toBe(true);
      expect(tui.phaseManager.transition).toHaveBeenCalledWith(InteractionPhase.Idle);
    });

    it("should reject on Esc during Approval", () => {
      const result = screenWithTui.handleInput!("\x1b");
      expect(result).toBe(true);
      expect(tui.phaseManager.transition).toHaveBeenCalledWith(InteractionPhase.Idle);
    });

    it("should approve on Enter during Approval", () => {
      const result = screenWithTui.handleInput!("\r");
      expect(result).toBe(true);
      expect(tui.phaseManager.transition).toHaveBeenCalledWith(InteractionPhase.Streaming);
    });
  });

  describe("startAgent", () => {
    it("should start agent and log starting message", async () => {
      const config = { workflowId: "test-workflow", maxIterations: 5 } as any;
      await screen.startAgent(config);

      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBeGreaterThan(0);
      expect(logEntries.some((e: any) => e.type === "system")).toBe(true);
    });

    it("should prevent starting multiple agents", async () => {
      const config = { workflowId: "test-workflow", maxIterations: 5 } as any;
      (screen as any).isRunning = true;

      await screen.startAgent(config);

      const logEntries = (screen as any).logEntries;
      expect(logEntries.some((e: any) => e.message.includes("already running"))).toBe(true);

      (screen as any).isRunning = false;
    });

    it("should update status to running after start", async () => {
      const config = { workflowId: "test-workflow", maxIterations: 5 } as any;
      await screen.startAgent(config);

      const container = screen.render() as Container;
      const statusLine = container.children[0] as Text;
      const rendered = statusLine.render(80);
      expect(rendered[0]).toContain("RUNNING");
    });

    it("should generate an agent ID on start", async () => {
      const config = { workflowId: "test-workflow", maxIterations: 5 } as any;
      await screen.startAgent(config);

      expect((screen as any).currentAgentId).toBeDefined();
      expect(typeof (screen as any).currentAgentId).toBe("string");
      expect((screen as any).isRunning).toBe(true);
    });
  });

  describe("sendMessage", () => {
    it("should append user message to log", async () => {
      (screen as any).isRunning = true;
      const sendMessage = (screen as any).sendMessage.bind(screen);
      await sendMessage("Test message");

      const logEntries = (screen as any).logEntries;
      const userMessage = logEntries.find((e: any) => e.type === "user");

      expect(userMessage).toBeDefined();
      expect(userMessage.message).toBe("Test message");
    });

    it("should prevent sending messages when agent not running", async () => {
      (screen as any).isRunning = false;
      const sendMessage = (screen as any).sendMessage.bind(screen);
      await sendMessage("Test message");

      const logEntries = (screen as any).logEntries;
      expect(logEntries.some((e: any) => e.message.includes("Start agent first"))).toBe(true);
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

    it("should limit log entries to LOG_MAX_ENTRIES", () => {
      const appendLog = (screen as any).appendLog.bind(screen);

      for (let i = 0; i < 110; i++) {
        appendLog(`Message ${i}`, "system");
      }

      const logEntries = (screen as any).logEntries;
      expect(logEntries.length).toBeLessThanOrEqual(100);
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
