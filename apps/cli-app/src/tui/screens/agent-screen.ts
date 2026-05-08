/**
 * Agent Loop Screen
 * Real-time agent execution monitoring with streaming logs
 */

import { Box, Container, Text, Input } from "../core/index.js";
import type { Screen } from "./screen.js";
import { AgentLoopAdapter } from "../../adapters/agent-loop-adapter.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";

interface LogEntry {
  timestamp: Date;
  type: "user" | "assistant" | "system" | "tool";
  message: string;
}

export class AgentScreen implements Screen {
  private container: Container;
  private statusPanel: Box;
  private logPanel: Box;
  private messageInput!: Input;
  private adapter: AgentLoopAdapter;
  private currentAgentId?: string;
  private isRunning: boolean = false;
  private logEntries: LogEntry[] = [];
  private onBack?: () => void;

  constructor(onBack?: () => void) {
    this.onBack = onBack;
    this.adapter = new AgentLoopAdapter();
    this.container = new Container();
    this.statusPanel = new Box();
    this.logPanel = new Box();
    
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

    // Log panel (scrollable area)
    const logBox = new Box();
    logBox.addChild(new Text("Execution Log:", 1, 0));
    logBox.addChild(this.logPanel);

    // Message input
    const inputBox = new Box();
    inputBox.addChild(new Text("Message:", 1, 0));
    this.messageInput = new Input("Enter your message...");
    this.messageInput.onSubmit = (text) => {
      if (text.trim()) {
        this.sendMessage(text);
      }
    };
    inputBox.addChild(this.messageInput);

    this.container.addChild(toolbar);
    this.container.addChild(statusBox);
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

  private appendLog(message: string, type: LogEntry["type"] = "system") {
    const entry: LogEntry = {
      timestamp: new Date(),
      type,
      message,
    };
    
    this.logEntries.push(entry);
    
    // Format log entry
    const timeStr = entry.timestamp.toLocaleTimeString();
    const typeIcon = {
      user: "👤",
      assistant: "🤖",
      system: "ℹ️",
      tool: "🔧",
    }[type];
    
    const formatted = `[${timeStr}] ${typeIcon} ${message}`;
    
    // Clear and rebuild log panel (simplified - in production would use virtual scrolling)
    this.logPanel.clear();
    this.logEntries.slice(-50).forEach(entry => {
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

  public async startAgent(config: AgentLoopRuntimeConfig) {
    if (this.isRunning) {
      this.appendLog("Agent is already running", "system");
      return;
    }

    this.isRunning = true;
    this.updateStatus("running");
    this.appendLog("Starting agent loop...", "system");

    try {
      const result = await this.adapter.executeAgentLoopStream(
        config,
        {},
        (event) => this.handleEvent(event)
      );

      this.isRunning = false;
      this.updateStatus(result.success ? "completed" : "error");
      
      if (result.success) {
        this.appendLog("Agent loop completed successfully", "system");
      } else {
        this.appendLog(`Agent loop failed: ${result.error}`, "system");
      }
    } catch (error) {
      this.isRunning = false;
      this.updateStatus("error");
      this.appendLog(
        `Error: ${error instanceof Error ? error.message : String(error)}`,
        "system"
      );
    }
  }

  private handleEvent(event: any) {
    switch (event.type) {
      case "text":
        if (event.delta) {
          this.appendLog(event.delta, "assistant");
        }
        break;
        
      case "tool_call_start":
        const toolName = event.data?.toolCall?.function?.name || "unknown";
        this.appendLog(`Calling tool: ${toolName}`, "tool");
        break;
        
      case "tool_call_end":
        const success = event.data?.success ? "✓" : "✗";
        this.appendLog(`${success} Tool call completed`, "tool");
        break;
        
      case "iteration_complete":
        const iteration = event.data?.iteration || 0;
        this.appendLog(`Iteration ${iteration} complete`, "system");
        break;
        
      case "user_message":
        this.appendLog(event.data?.content || "", "user");
        break;
        
      default:
        this.appendLog(`Event: ${event.type}`, "system");
    }
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
    // For now, just log it
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
