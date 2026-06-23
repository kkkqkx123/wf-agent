/**
 * Agent Loop Screen
 * Real-time agent execution monitoring with streaming logs
 */

import { Box, Container, Text, Input } from "../core/index.js";
import type { Screen } from "./screen.js";
import type { Component } from "../core/tui.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import { IterationPanel } from "../components/iteration-panel.js";
import { ToolCallIndicator } from "../components/tool-call-indicator.js";

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

  constructor(onBack?: () => void) {
    this.onBack = onBack;
    this.container = new Container();
    this.statusPanel = new Box();
    this.logPanel = new Box();
    this.iterationPanel = new IterationPanel({ maxHeight: 8 });
    this.toolCallPanel = new ToolCallIndicator({ maxDisplayCalls: 5, showArguments: false });

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

    // Iteration panel (new)
    const iterationBox = new Box();
    iterationBox.addChild(new Text("Iterations:", 1, 0));
    iterationBox.addChild(this.iterationPanel as unknown as Component);

    // Tool call panel (new)
    const toolCallBox = new Box();
    toolCallBox.addChild(new Text("Tool Calls:", 1, 0));
    toolCallBox.addChild(this.toolCallPanel as unknown as Component);

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

    this.statusPanel.addChild(new Text(`Status: ${statusIcon} ${status.toUpperCase()}`, 1, 0));
    this.statusPanel.addChild(new Text(`Agent ID: ${this.currentAgentId || "N/A"}`, 1, 0));
    this.statusPanel.addChild(new Text(`Messages: ${this.logEntries.length}`, 1, 0));
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

    if (options?.stream) {
      // For streaming, append without full re-render (performance optimization)
      // In a real implementation, this would use virtual scrolling or append-only updates
      this.logEntries.push(entry);

      // Only keep last 100 entries for performance
      if (this.logEntries.length > 100) {
        this.logEntries.shift();
      }

      // Append to log panel without full rebuild
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
      // For non-streaming, add as new entry and rebuild
      this.logEntries.push(entry);

      // Keep only last 50 entries for performance
      if (this.logEntries.length > 50) {
        this.logEntries.shift();
      }

      // Clear and rebuild log panel
      this.logPanel.clear();
      this.logEntries.forEach(entry => {
        const timeStr = entry.timestamp.toLocaleTimeString();
        const typeIcon = {
          user: "👤",
          assistant: "🤖",
          system: "ℹ️",
          tool: "🔧",
        }[entry.type];

        this.logPanel.addChild(new Text(`[${timeStr}] ${typeIcon} ${entry.message}`, 1, 0));
      });
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
    // Handle toolbar shortcuts
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

    // Delegate to message input when focused
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