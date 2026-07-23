/**
 * Agent Loop Screen
 * Real-time agent execution monitoring with streaming logs
 */

import { Box, Container, Text, Input, InputMode } from "../core/index.js";
import { getKeybindings } from "../core/keybindings.js";
import type { Screen } from "./screen.js";
import type { Component } from "../core/tui.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import { IterationPanel } from "../components/iteration-panel.js";
import { ToolCallIndicator } from "../components/tool-call-indicator.js";
import { FoldableSection } from "../components/foldable-section.js";
import type { TUI } from "../core/tui.js";

const NORMAL_MODE_MAX_LOG_LINES = 20;

interface LogEntry {
  timestamp: Date;
  type: "user" | "assistant" | "system" | "tool";
  message: string;
}

export class AgentScreen implements Screen {
  private container: Container;
  private statusPanel: Box;
  private logPanel: Box;
  private iterationPanel: IterationPanel;
  private toolCallPanel: ToolCallIndicator;
  private messageInput!: Input;
  private currentAgentId?: string;
  private isRunning: boolean = false;
  private logEntries: LogEntry[] = [];
  private onBack?: () => void;

  /** Current input mode: Chat (insert) or Normal (browse) */
  private mode: InputMode = InputMode.Chat;

  /** Scroll offset for Normal mode log browsing */
  private scrollOffset: number = 0;

  /** Reference to TUI for mode-aware input routing */
  private tui: TUI;

  /** Foldable sections for collapsible panels */
  private foldableSections: FoldableSection[] = [];

  constructor(onBack?: () => void, tui?: TUI) {
    this.onBack = onBack;
    this.tui = tui!;
    this.container = new Container();
    this.statusPanel = new Box();
    this.logPanel = new Box();
    this.iterationPanel = new IterationPanel({ maxHeight: 8 });
    this.toolCallPanel = new ToolCallIndicator({ maxDisplayCalls: 5 });

    this.setupLayout();
  }

  private setupLayout() {
    // Toolbar
    const toolbar = new Box();
    toolbar.addChild(new Text("[S]tart  [P]ause  [R]esume  [C]ancel  [B]ack", 1, 0));

    // Status panel
    const statusBox = new Box();
    statusBox.addChild(new Text("Agent Status:", 1, 0));
    statusBox.addChild(this.statusPanel);
    this.updateStatus("idle");

    // Iteration panel (new, wrapped in foldable section)
    const iterationBox = new Box();
    iterationBox.addChild(new Text("Iterations:", 1, 0));
    const foldableIterations = new FoldableSection(
      "iterations",
      "Iterations - agent iteration progress",
      this.iterationPanel as unknown as Component,
      { collapsed: false },
    );
    this.foldableSections.push(foldableIterations);
    iterationBox.addChild(foldableIterations);

    // Tool call panel (new, wrapped in foldable section)
    const toolCallBox = new Box();
    toolCallBox.addChild(new Text("Tool Calls:", 1, 0));
    const foldableToolCalls = new FoldableSection(
      "toolCalls",
      "Tool Calls - active and recent tool invocations",
      this.toolCallPanel as unknown as Component,
      { collapsed: false },
    );
    this.foldableSections.push(foldableToolCalls);
    toolCallBox.addChild(foldableToolCalls);

    // Log panel (scrollable area)
    const logBox = new Box();
    logBox.addChild(new Text("Execution Log:", 1, 0));
    logBox.addChild(this.logPanel);

    // Message input
    const inputBox = new Box();
    inputBox.addChild(new Text("Message:", 1, 0));
    this.messageInput = new Input("Enter your message...");
    this.messageInput.onSubmit = text => {
      if (text.trim()) {
        this.sendMessage(text);
      }
    };
    inputBox.addChild(this.messageInput);

    this.container.addChild(toolbar);
    this.container.addChild(statusBox);
    this.container.addChild(iterationBox);
    this.container.addChild(toolCallBox);
    this.container.addChild(logBox);
    this.container.addChild(inputBox);
  }

  private updateStatus(status: "idle" | "running" | "paused" | "completed" | "error") {
    this.statusPanel.clear();

    const statusIcon = {
      idle: "⏸️",
      running: "▶️",
      paused: "⏸️",
      completed: "✅",
      error: "❌",
    }[status];

    const modeLabel = this.mode === InputMode.Normal ? " [NORMAL]" : "";

    this.statusPanel.addChild(new Text(`Status: ${statusIcon} ${status.toUpperCase()}${modeLabel}`, 1, 0));
    this.statusPanel.addChild(new Text(`Agent ID: ${this.currentAgentId || "N/A"}`, 1, 0));
    this.statusPanel.addChild(new Text(`Messages: ${this.logEntries.length}`, 1, 0));
  }

  /**
   * Rebuild log panel respecting scrollOffset (Normal mode) or showing latest (Chat mode).
   */
  private rebuildLogPanel(): void {
    this.logPanel.clear();

    if (this.mode === InputMode.Normal) {
      const totalEntries = this.logEntries.length;
      const endIndex = Math.min(totalEntries, this.scrollOffset + NORMAL_MODE_MAX_LOG_LINES);
      const startIndex = Math.max(0, this.scrollOffset);

      // Scroll indicator at top
      if (startIndex > 0) {
        this.logPanel.addChild(new Text(`--- ${startIndex} older entries above (j/k to scroll) ---`, 1, 0));
      }

      for (let i = startIndex; i < endIndex && i < totalEntries; i++) {
        const entry = this.logEntries[i];
        if (!entry) continue;
        const timeStr = entry.timestamp.toLocaleTimeString();
        const typeIcon = {
          user: "👤",
          assistant: "🤖",
          system: "ℹ️",
          tool: "🔧",
        }[entry.type];
        this.logPanel.addChild(new Text(`[${timeStr}] ${typeIcon} ${entry.message}`, 1, 0));
      }

      // Scroll indicator at bottom
      if (endIndex < totalEntries) {
        this.logPanel.addChild(new Text(`--- ${totalEntries - endIndex} newer entries below ---`, 1, 0));
      }
    } else {
      // Chat mode: show latest entries
      const startIndex = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
      for (let i = startIndex; i < this.logEntries.length; i++) {
        const entry = this.logEntries[i];
        if (!entry) continue;
        const timeStr = entry.timestamp.toLocaleTimeString();
        const typeIcon = {
          user: "👤",
          assistant: "🤖",
          system: "ℹ️",
          tool: "🔧",
        }[entry.type];
        this.logPanel.addChild(new Text(`[${timeStr}] ${typeIcon} ${entry.message}`, 1, 0));
      }
    }
  }

  private appendLog(
    message: string,
    type: LogEntry["type"] = "system",
    options?: { stream?: boolean },
  ) {
    const entry: LogEntry = {
      timestamp: new Date(),
      type,
      message,
    };

    this.logEntries.push(entry);

    // Keep only last 100 entries for performance
    if (this.logEntries.length > 100) {
      this.logEntries.shift();
    }

    if (options?.stream) {
      // For streaming, append without full re-render (performance optimization)
      const timeStr = entry.timestamp.toLocaleTimeString();
      const typeIcon = {
        user: "👤",
        assistant: "🤖",
        system: "ℹ️",
        tool: "🔧",
      }[type];
      const formatted = `[${timeStr}] ${typeIcon} ${message}`;
      this.logPanel.addChild(new Text(formatted, 1, 0));
    } else {
      // Auto-scroll in Chat mode (follow latest)
      if (this.mode === InputMode.Chat) {
        this.scrollOffset = 0;
      }
      this.rebuildLogPanel();
    }
  }

  public async startAgent(_config: AgentLoopRuntimeConfig) {
    if (this.isRunning) {
      this.appendLog("Agent is already running", "system");
      return;
    }

    // Generate agent ID
    this.currentAgentId = `agent-${Date.now()}-${Math.random().toString(7)}`;

    this.isRunning = true;
    this.updateStatus("running");
    this.appendLog(`Starting agent loop (${this.currentAgentId})...`, "system");

    // Agent loop execution should be triggered externally via SDK adapter
  }

  private async sendMessage(text: string) {
    if (!this.isRunning) {
      this.appendLog("Start agent first before sending messages", "system");
      return;
    }

    this.appendLog(text, "user");

    // Clear input
    this.messageInput.setValue("");

    // In a real implementation, this would send the message to the running agent
    this.appendLog("Message sent (integration pending)", "system");
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    const kb = getKeybindings();

    // Handle toolbar shortcuts (available in both modes)
    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }

    if (data === "s" || data === "S") {
      // TODO: Show configuration dialog and start agent
      this.appendLog("Start agent - configuration dialog to be implemented", "system");
      return true;
    }

    if (data === "c" || data === "C") {
      this.isRunning = false;
      this.updateStatus("idle");
      this.appendLog("Agent cancelled", "system");
      return true;
    }

    // === Normal mode: vim-style log navigation ===
    if (this.mode === InputMode.Normal) {
      // Space — toggle all foldable sections
      if (data === " ") {
        const anyCollapsed = this.foldableSections.some((s) => s.isCollapsed());
        for (const section of this.foldableSections) {
          section.setCollapsed(!anyCollapsed);
        }
        this.rebuildLogPanel();
        return true;
      }

      // Enter, Esc, or any printable char → switch back to Chat mode
      if (kb.matches(data, "tui.input.submit")) {
        this.mode = InputMode.Chat;
        this.scrollOffset = 0;
        this.rebuildLogPanel();
        if (this.tui) this.tui.setInputMode(InputMode.Chat);
        if (this.tui) this.tui.setContext("chat");
        return true;
      }

      if (kb.matches(data, "tui.select.cancel")) {
        // Esc in Normal mode also returns to Chat
        this.mode = InputMode.Chat;
        this.scrollOffset = 0;
        this.rebuildLogPanel();
        if (this.tui) this.tui.setInputMode(InputMode.Chat);
        if (this.tui) this.tui.setContext("chat");
        return true;
      }

      // j / Down — scroll down (toward newer entries)
      if (kb.matches(data, "tui.navigate.up")) {
        this.scrollOffset = Math.max(0, this.scrollOffset - 1);
        this.rebuildLogPanel();
        return true;
      }

      // k / Up — scroll up (toward older entries)
      if (kb.matches(data, "tui.navigate.down")) {
        const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
        this.scrollOffset = Math.min(maxOffset, this.scrollOffset + 1);
        this.rebuildLogPanel();
        return true;
      }

      // Ctrl+u — scroll up half page
      if (kb.matches(data, "tui.navigate.halfPageUp")) {
        const halfPage = Math.max(1, Math.floor(NORMAL_MODE_MAX_LOG_LINES / 2));
        const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
        this.scrollOffset = Math.min(maxOffset, this.scrollOffset + halfPage);
        this.rebuildLogPanel();
        return true;
      }

      // Ctrl+d — scroll down half page
      if (kb.matches(data, "tui.navigate.halfPageDown")) {
        const halfPage = Math.max(1, Math.floor(NORMAL_MODE_MAX_LOG_LINES / 2));
        this.scrollOffset = Math.max(0, this.scrollOffset - halfPage);
        this.rebuildLogPanel();
        return true;
      }

      // g / Ctrl+Home — jump to top (oldest entries)
      if (kb.matches(data, "tui.navigate.top")) {
        const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
        this.scrollOffset = maxOffset;
        this.rebuildLogPanel();
        return true;
      }

      // G / Ctrl+End — jump to bottom (latest entries)
      if (kb.matches(data, "tui.navigate.bottom")) {
        this.scrollOffset = 0;
        this.rebuildLogPanel();
        return true;
      }

      // Any printable character → switch to Chat mode
      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        this.mode = InputMode.Chat;
        this.scrollOffset = 0;
        if (this.tui) this.tui.setInputMode(InputMode.Chat);
        if (this.tui) this.tui.setContext("chat");
        // Forward the character to the input
        if (this.messageInput.handleInput) {
          this.messageInput.handleInput(data);
        }
        this.rebuildLogPanel();
        return true;
      }

      return true; // Consume all other input in Normal mode
    }

    // === Chat mode: handle Esc → switch to Normal ===
    if (kb.matches(data, "tui.select.cancel")) {
      this.mode = InputMode.Normal;
      this.scrollOffset = 0;
      this.rebuildLogPanel();
      if (this.tui) this.tui.setInputMode(InputMode.Normal);
      return true;
    }

    // === Chat mode: delegate to message input ===
    if (this.messageInput.handleInput) {
      this.messageInput.handleInput(data);
      return true;
    }

    return false;
  }

  destroy(): void {
    // Cleanup running agent if needed
    if (this.isRunning) {
      this.isRunning = false;
    }
  }
}