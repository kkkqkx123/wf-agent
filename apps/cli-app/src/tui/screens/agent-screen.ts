/**
 * Agent Loop Screen
 *
 * Real-time agent monitoring with flat layout:
 * - Single-line status at top
 * - Log fills available space
 * - Input at bottom
 *
 * No foldable sections, no side panels.
 */

import { Container, Text, Input, InteractionPhase } from "../core/index.js";
import { getKeybindings } from "../core/keybindings.js";
import type { Screen } from "./screen.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import type { TUI } from "../core/tui.js";

interface LogEntry {
  timestamp: Date;
  type: "user" | "assistant" | "system" | "tool";
  message: string;
}

const LOG_MAX_ENTRIES = 100;
const LOG_VISIBLE_LINES = 20;

export class AgentScreen implements Screen {
  private container: Container;
  private statusLine: Text;
  private logContainer: Container;
  private messageInput!: Input;
  private currentAgentId?: string;
  private isRunning: boolean = false;
  private logEntries: LogEntry[] = [];
  private onBack?: () => void;
  private scrollOffset: number = 0;
  private tui: TUI;

  constructor(onBack?: () => void, tui?: TUI) {
    this.onBack = onBack;
    this.tui = tui!;
    this.container = new Container();
    this.statusLine = new Text("", 0, 0);
    this.logContainer = new Container();
    this.setupLayout();
  }

  private setupLayout() {
    this.messageInput = new Input("Type your message...");
    this.messageInput.onSubmit = text => {
      if (text.trim()) this.sendMessage(text);
    };

    this.container.addChild(this.statusLine);
    this.container.addChild(this.logContainer);
    this.container.addChild(this.messageInput);

    this.updateStatus("idle");
  }

  private updateStatus(status: "idle" | "running" | "paused" | "completed" | "error") {
    const icons: Record<string, string> = {
      idle: "⏸", running: "▶", paused: "⏸", completed: "✅", error: "❌",
    };
    const phaseLabel = this.getPhaseLabel();
    const agentId = this.currentAgentId || "N/A";
    const msgCount = this.logEntries.length;

    this.statusLine.setText(
      `${icons[status]} ${status.toUpperCase()}${phaseLabel}  ${agentId}  msgs:${msgCount}`,
    );
  }

  private getPhaseLabel(): string {
    switch (this.tui?.phase) {
      case InteractionPhase.Streaming: return " [STREAMING]";
      case InteractionPhase.Approval: return " [APPROVAL]";
      case InteractionPhase.Normal: return " [NORMAL]";
      default: return "";
    }
  }

  private rebuildLog(): void {
    this.logContainer.clear();

    const phase = this.tui?.phase;
    let startIndex: number;

    if (phase === InteractionPhase.Normal) {
      startIndex = Math.max(0, this.scrollOffset);
    } else {
      startIndex = Math.max(0, this.logEntries.length - LOG_VISIBLE_LINES);
    }

    const endIndex = Math.min(this.logEntries.length, startIndex + LOG_VISIBLE_LINES);

    if (startIndex > 0) {
      this.logContainer.addChild(new Text(`--- ${startIndex} older ---`));
    }

    for (let i = startIndex; i < endIndex; i++) {
      const entry = this.logEntries[i];
      if (!entry) continue;
      const timeStr = entry.timestamp.toLocaleTimeString();
      const icons: Record<string, string> = { user: "👤", assistant: "🤖", system: "ℹ", tool: "🔧" };
      this.logContainer.addChild(new Text(`[${timeStr}] ${icons[entry.type]} ${entry.message}`));
    }

    if (endIndex < this.logEntries.length) {
      const remaining = this.logEntries.length - endIndex;
      this.logContainer.addChild(new Text(`--- ${remaining} newer ---`));
    }
  }

  private appendLog(message: string, type: LogEntry["type"] = "system") {
    this.logEntries.push({ timestamp: new Date(), type, message });

    if (this.logEntries.length > LOG_MAX_ENTRIES) {
      this.logEntries.shift();
    }

    if (this.tui?.phase !== InteractionPhase.Normal) {
      this.scrollOffset = 0;
    }

    this.rebuildLog();
    this.updateStatus(this.isRunning ? "running" : "idle");
    this.tui?.requestRender();
  }

  public async startAgent(_config: AgentLoopRuntimeConfig) {
    if (this.isRunning) {
      this.appendLog("Agent is already running");
      return;
    }

    this.currentAgentId = `agent-${Date.now()}-${Math.random().toString(7)}`;
    this.isRunning = true;
    this.updateStatus("running");
    this.appendLog(`Starting agent loop (${this.currentAgentId})...`);
  }

  private async sendMessage(text: string) {
    if (!this.isRunning) {
      this.appendLog("Start agent first");
      return;
    }

    this.appendLog(text, "user");
    this.messageInput.setValue("");
    this.appendLog("Message sent (integration pending)");
  }

  render(): Container {
    this.rebuildLog();
    return this.container;
  }

  handleInput(data: string): boolean {
    const kb = getKeybindings();

    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }

    if (data === "s" || data === "S") {
      this.appendLog("Start agent - configuration dialog to be implemented");
      return true;
    }

    switch (this.tui?.phase) {
      case InteractionPhase.Streaming: {
        if (data === "\x03") {
          this.cancelTurn();
          return true;
        }
        return true;
      }

      case InteractionPhase.Approval: {
        if (data === "\x03") {
          this.cancelTurn();
          return true;
        }
        if (data === "y" || data === "Y" || data === "\r") {
          this.tui?.phaseManager.transition(InteractionPhase.Streaming);
          this.updateStatus("running");
          this.appendLog("Tool call approved");
          return true;
        }
        if (data === "n" || data === "N" || data === "\x1b") {
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          this.updateStatus("idle");
          this.appendLog("Tool call rejected");
          return true;
        }
        return true;
      }

      case InteractionPhase.Normal: {
        if (data === " ") {
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          return true;
        }

        if (kb.matches(data, "tui.input.submit") || kb.matches(data, "tui.select.cancel")) {
          this.scrollOffset = 0;
          this.rebuildLog();
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          return true;
        }

        if (kb.matches(data, "tui.navigate.up")) {
          this.scrollOffset = Math.max(0, this.scrollOffset - 1);
          this.rebuildLog();
          return true;
        }

        if (kb.matches(data, "tui.navigate.down")) {
          const maxOff = Math.max(0, this.logEntries.length - LOG_VISIBLE_LINES);
          this.scrollOffset = Math.min(maxOff, this.scrollOffset + 1);
          this.rebuildLog();
          return true;
        }

        if (kb.matches(data, "tui.navigate.halfPageUp")) {
          const half = Math.max(1, Math.floor(LOG_VISIBLE_LINES / 2));
          const maxOff = Math.max(0, this.logEntries.length - LOG_VISIBLE_LINES);
          this.scrollOffset = Math.min(maxOff, this.scrollOffset + half);
          this.rebuildLog();
          return true;
        }

        if (kb.matches(data, "tui.navigate.halfPageDown")) {
          const half = Math.max(1, Math.floor(LOG_VISIBLE_LINES / 2));
          this.scrollOffset = Math.max(0, this.scrollOffset - half);
          this.rebuildLog();
          return true;
        }

        if (kb.matches(data, "tui.navigate.top")) {
          const maxOff = Math.max(0, this.logEntries.length - LOG_VISIBLE_LINES);
          this.scrollOffset = maxOff;
          this.rebuildLog();
          return true;
        }

        if (kb.matches(data, "tui.navigate.bottom")) {
          this.scrollOffset = 0;
          this.rebuildLog();
          return true;
        }

        if (data.length === 1 && data.charCodeAt(0) >= 32) {
          this.scrollOffset = 0;
          this.tui?.phaseManager.transition(InteractionPhase.Idle);
          this.messageInput.handleInput?.(data);
          this.rebuildLog();
          return true;
        }

        return true;
      }

      default: {
        if (kb.matches(data, "tui.select.cancel")) {
          this.scrollOffset = 0;
          this.rebuildLog();
          this.tui?.phaseManager.transition(InteractionPhase.Normal);
          return true;
        }

        return this.messageInput.handleInput?.(data), true;
      }
    }
  }

  private cancelTurn(): void {
    this.tui?.phaseManager.transition(InteractionPhase.Idle);
    this.tui?.phaseManager.clearBufferedInput();
    this.isRunning = false;
    this.updateStatus("idle");
    this.appendLog("Turn cancelled");
  }

  destroy(): void {
    if (this.isRunning) this.isRunning = false;
  }
}
