/**
 * Agent Loop Screen
 * Real-time agent execution monitoring with streaming logs
 */

import { Box, Container, Text, Input } from "../core/index.js";
import type { Screen } from "./screen.js";
import type { Component } from "../core/tui.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import type { MessageBus, MessageSubscription } from "@wf-agent/sdk";
import { MessageCategory, AgentMessageType } from "@wf-agent/types";
import type {
  AgentStartData,
  AgentEndData,
  AgentIterationData,
  AgentLLMStreamData,
  AgentToolCallData,
  AgentToolEndData,
  BaseComponentMessage,
} from "@wf-agent/types";
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
  private messageBus?: MessageBus;
  private currentAgentId?: string;
  private isRunning: boolean = false;
  private logEntries: LogEntry[] = [];
  private onBack?: () => void;
  private subscriptions: MessageSubscription[] = [];
  private streamingBuffer: string = "";
  private lastRenderTime: number = 0;

  constructor(messageBus?: MessageBus, onBack?: () => void) {
    this.messageBus = messageBus;
    this.onBack = onBack;
    this.container = new Container();
    this.statusPanel = new Box();
    this.logPanel = new Box();
    this.iterationPanel = new IterationPanel({ maxHeight: 8 });
    this.toolCallPanel = new ToolCallIndicator({ maxDisplayCalls: 5, showArguments: false });
    
    this.setupLayout();
    this.setupMessageSubscriptions();
  }

  /**
   * Setup message subscriptions for real-time agent event updates
   */
  private setupMessageSubscriptions() {
    if (!this.messageBus) return;

    // Subscribe to all agent messages for this instance (granular filtering)
    if (this.currentAgentId) {
      const agentSubscription = this.messageBus.subscribe(
        {
          categories: [MessageCategory.AGENT],
          entityIds: [this.currentAgentId],
        },
        (message) => this.handleAgentMessage(message)
      );
      this.subscriptions.push(agentSubscription);
    } else {
      // Fallback: subscribe to all agent messages if no specific agent ID
      const lifecycleSubscription = this.messageBus.subscribe(
        {
          categories: [MessageCategory.AGENT],
          types: [
            AgentMessageType.START,
            AgentMessageType.END,
            AgentMessageType.PAUSE,
            AgentMessageType.RESUME,
            AgentMessageType.CANCEL,
          ],
        },
        (message) => this.handleAgentLifecycleMessage(message)
      );
      this.subscriptions.push(lifecycleSubscription);

      // Subscribe to iteration events
      const iterationSubscription = this.messageBus.subscribe(
        {
          categories: [MessageCategory.AGENT],
          types: [
            AgentMessageType.ITERATION_START,
            AgentMessageType.ITERATION_END,
          ],
        },
        (message) => this.handleIterationMessage(message)
      );
      this.subscriptions.push(iterationSubscription);

      // Subscribe to LLM streaming events
      const llmSubscription = this.messageBus.subscribe(
        {
          categories: [MessageCategory.AGENT],
          types: [AgentMessageType.LLM_STREAM],
        },
        (message) => this.handleLLMStreamMessage(message)
      );
      this.subscriptions.push(llmSubscription);

      // Subscribe to tool execution events
      const toolSubscription = this.messageBus.subscribe(
        {
          categories: [MessageCategory.AGENT],
          types: [
            AgentMessageType.TOOL_CALL_START,
            AgentMessageType.TOOL_CALL_END,
          ],
        },
        (message) => this.handleToolMessage(message)
      );
      this.subscriptions.push(toolSubscription);
    }
  }

  /**
   * Unified agent message handler (for entity-filtered subscriptions)
   */
  private handleAgentMessage(message: BaseComponentMessage) {
    switch (message.type) {
      case AgentMessageType.START:
        this.handleAgentLifecycleMessage(message);
        break;
      case AgentMessageType.END:
      case AgentMessageType.PAUSE:
      case AgentMessageType.RESUME:
      case AgentMessageType.CANCEL:
        this.handleAgentLifecycleMessage(message);
        break;
      case AgentMessageType.ITERATION_START:
      case AgentMessageType.ITERATION_END:
        this.handleIterationMessage(message);
        break;
      case AgentMessageType.LLM_STREAM:
        this.handleLLMStreamMessage(message);
        break;
      case AgentMessageType.TOOL_CALL_START:
      case AgentMessageType.TOOL_CALL_END:
        this.handleToolMessage(message);
        break;
    }
  }

  /**
   * Handle agent lifecycle messages
   */
  private handleAgentLifecycleMessage(message: BaseComponentMessage) {
    switch (message.type) {
      case AgentMessageType.START:
        {
          const startData = message.data as AgentStartData;
          this.currentAgentId = startData.loopId;
          this.isRunning = true;
          this.updateStatus("running");
          this.appendLog(`Agent started: ${startData.agentId}`, "system");
        }
        break;

      case AgentMessageType.END:
        {
          const endData = message.data as AgentEndData;
          this.isRunning = false;
          this.updateStatus(endData.status === "completed" ? "completed" : "error");
          this.appendLog(
            `Agent ended: ${endData.status} (${endData.totalIterations} iterations, ${endData.duration}ms)`,
            "system"
          );
        }
        break;

      case AgentMessageType.PAUSE:
        this.updateStatus("paused");
        this.appendLog("Agent paused", "system");
        break;

      case AgentMessageType.RESUME:
        this.updateStatus("running");
        this.appendLog("Agent resumed", "system");
        break;

      case AgentMessageType.CANCEL:
        this.isRunning = false;
        this.updateStatus("idle");
        this.appendLog("Agent cancelled", "system");
        break;
    }
  }

  /**
   * Handle iteration messages
   */
  private handleIterationMessage(message: BaseComponentMessage) {
    const data = message.data as AgentIterationData;
    
    if (message.type === AgentMessageType.ITERATION_START) {
      this.iterationPanel.updateIteration(data);
      this.appendLog(`Iteration ${data.iteration} started`, "system");
    } else if (message.type === AgentMessageType.ITERATION_END) {
      this.iterationPanel.completeIteration(data.iteration, data.duration);
      this.appendLog(
        `Iteration ${data.iteration} completed${data.duration ? ` (${data.duration}ms)` : ""}`,
        "system"
      );
    }
  }

  /**
   * Handle LLM stream messages with optimized streaming performance
   */
  private handleLLMStreamMessage(message: BaseComponentMessage) {
    const data = message.data as AgentLLMStreamData;
    if (data.chunk) {
      // Buffer streaming chunks for performance
      this.streamingBuffer += data.chunk;
      
      const now = Date.now();
      // Render every 100ms or when buffer is large enough
      if (now - this.lastRenderTime > 100 || this.streamingBuffer.length > 200) {
        this.appendLog(this.streamingBuffer, "assistant", { stream: true });
        this.streamingBuffer = "";
        this.lastRenderTime = now;
      }
    }
  }

  /**
   * Handle tool execution messages
   */
  private handleToolMessage(message: BaseComponentMessage) {
    if (message.type === AgentMessageType.TOOL_CALL_START) {
      const data = message.data as AgentToolCallData;
      this.toolCallPanel.handleToolCallStart(data);
      this.appendLog(`Calling tool: ${data.toolName}`, "tool");
    } else if (message.type === AgentMessageType.TOOL_CALL_END) {
      const data = message.data as AgentToolEndData;
      this.toolCallPanel.handleToolCallEnd(data);
      const success = data.success ? "✓" : "✗";
      this.appendLog(`${success} Tool ${data.toolName} completed (${data.duration}ms)`, "tool");
    }
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
    this.messageInput.onSubmit = (text) => {
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

  private appendLog(message: string, type: LogEntry["type"] = "system", options?: { stream?: boolean }) {
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
    this.currentAgentId = `agent-${Date.now()}-${Math.random().toString(36).substring(7)}`;
    
    // Clear old subscriptions and setup new ones with entity filtering
    this.subscriptions.forEach(sub => sub.unsubscribe());
    this.subscriptions = [];
    this.setupMessageSubscriptions();

    this.isRunning = true;
    this.updateStatus("running");
    this.appendLog(`Starting agent loop (${this.currentAgentId})...`, "system");

    // Note: Agent execution is handled by SDK via message bus
    // This screen subscribes to messages and displays updates in real-time
    // The actual agent loop should be started by publishing an AGENT_START message
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
    // Cleanup subscriptions
    this.subscriptions.forEach(subscription => subscription.unsubscribe());
    this.subscriptions = [];
    
    // Cleanup running agent if needed
    if (this.isRunning) {
      this.isRunning = false;
    }
  }
}
