/**
 * Agent Loop Screen
 * Real-time agent execution monitoring with streaming logs
 */

import { Box, Container, Text, Input, InteractionPhase } from "../core/index.js";
import { getKeybindings } from "../core/keybindings.js";
import type { Screen } from "./screen.js";
import type { Component } from "../core/tui.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import { IterationPanel } from "../components/iteration-panel.js";
import { ToolCallIndicator } from "../components/tool-call-indicator.js";
import type { TUI } from "../core/tui.js";
import { FoldableSection } from "../components/foldable-section.js";

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

    const phaseLabel = this.getPhaseLabel();

    this.statusPanel.addChild(new Text(`Status: ${statusIcon} ${status.toUpperCase()}${phaseLabel}`, 1, 0));
    this.statusPanel.addChild(new Text(`Agent ID: ${this.currentAgentId || "N/A"}`, 1, 0));
    this.statusPanel.addChild(new Text(`Messages: ${this.logEntries.length}`, 1, 0));
  }

  private getPhaseLabel(): string {
    switch (this.tui?.phase) {
      case InteractionPhase.Streaming: return " [STREAMING]";
      case InteractionPhase.Approval: return " [APPROVAL]";
      case InteractionPhase.Normal: return " [NORMAL]";
      default: return "";
    }
  }

  /**
   * Rebuild log panel respecting scrollOffset (Normal mode) or showing latest (Chat mode).
   */
  private rebuildLogPanel(): void {
    this.logPanel.clear();

    if (this.tui?.phase === InteractionPhase.Normal) {
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
      if (this.tui?.phase !== InteractionPhase.Normal) {
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

    // Handle toolbar shortcuts (available in all phases)
    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }

    if (data === "s" || data === "S") {
      this.appendLog("Start agent - configuration dialog to be implemented", "system");
      return true;
    }

    // === Phase-specific routing ===
    switch (this.tui?.phase) {
      case InteractionPhase.Streaming: {
        if (data === "\x03") {
          this.cancelTurn();
          return true;
        }
        if (data === "\x0f") {
          this.appendLog("Tool skip requested (pending implementation)", "system");
          return true;
        }
        return true;
      }

      case InteractionPhase.Approval: {
        if (data === "\x03") {
          this.cancelTurn();
          return true;
        }
        if (data === "\x0f") {
          this.appendLog("Tool skip requested (pending implementation)", "system");
          return true;
        }
        if (data === "y" || data === "Y" || data === "\r") {
          this.tui?.phaseManager.transition(InteractionPhase.Streaming);
          this.updateStatus("running");
          this.appendLog("Tool call approved", "system");
          return true;
        }
        if (data === "n" || data === "N" || data === "\x1b") {
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          this.updateStatus("idle");
          this.appendLog("Tool call rejected", "system");
          return true;
        }
        return true;
      }

      case InteractionPhase.Normal: {
        // Space — toggle all foldable sections
        if (data === " ") {
          const anyCollapsed = this.foldableSections.some((s) => s.isCollapsed());
          for (const section of this.foldableSections) {
            section.setCollapsed(!anyCollapsed);
          }
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.input.submit")) {
          this.scrollOffset = 0;
          this.rebuildLogPanel();
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          return true;
        }

        if (kb.matches(data, "tui.select.cancel")) {
          this.scrollOffset = 0;
          this.rebuildLogPanel();
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          return true;
        }

        if (kb.matches(data, "tui.navigate.up")) {
          this.scrollOffset = Math.max(0, this.scrollOffset - 1);
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.navigate.down")) {
          const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
          this.scrollOffset = Math.min(maxOffset, this.scrollOffset + 1);
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.navigate.halfPageUp")) {
          const halfPage = Math.max(1, Math.floor(NORMAL_MODE_MAX_LOG_LINES / 2));
          const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
          this.scrollOffset = Math.min(maxOffset, this.scrollOffset + halfPage);
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.navigate.halfPageDown")) {
          const halfPage = Math.max(1, Math.floor(NORMAL_MODE_MAX_LOG_LINES / 2));
          this.scrollOffset = Math.max(0, this.scrollOffset - halfPage);
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.navigate.top")) {
          const maxOffset = Math.max(0, this.logEntries.length - NORMAL_MODE_MAX_LOG_LINES);
          this.scrollOffset = maxOffset;
          this.rebuildLogPanel();
          return true;
        }

        if (kb.matches(data, "tui.navigate.bottom")) {
          this.scrollOffset = 0;
          this.rebuildLogPanel();
          return true;
        }

        if (data.length === 1 && data.charCodeAt(0) >= 32) {
          this.scrollOffset = 0;
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          if (this.messageInput.handleInput) {
            this.messageInput.handleInput(data);
          }
          this.rebuildLogPanel();
          return true;
        }

        return true;
      }

      default: {
        if (kb.matches(data, "tui.select.cancel")) {
          this.scrollOffset = 0;
          this.rebuildLogPanel();
          this.tui?.phaseManager.transition(InteractionPhase.Normal);
          return true;
        }

        if (this.messageInput.handleInput) {
          this.messageInput.handleInput(data);
          return true;
        }

        return false;
      }
    }
  }

  private cancelTurn(): void {
    this.tui?.phaseManager.transition(InteractionPhase.Idle);
    this.tui?.phaseManager.clearBufferedInput();
    this.isRunning = false;
    this.updateStatus("idle");
    this.appendLog("Turn cancelled", "system");
  }

  destroy(): void {
    // Cleanup running agent if needed
    if (this.isRunning) {
      this.isRunning = false;
    }
  }
}