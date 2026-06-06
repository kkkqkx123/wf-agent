/**
 * Dashboard screen - main menu and status overview
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import { MessageCategory, AgentMessageType, WorkflowExecutionMessageType } from "@wf-agent/types";
import type { BaseComponentMessage } from "@wf-agent/types";

export class DashboardScreen implements Screen {
  private container: Container;
  private menuList!: SelectList;
  private statusPanel!: Box;
  private onNavigate?: (screenId: string) => void;
  private activeAgents: number = 0;
  private runningThreads: number = 0;

  constructor(onNavigate?: (screenId: string) => void) {
    this.onNavigate = onNavigate;
    this.container = new Container();
    this.setupLayout();
  }

  private setupLayout() {
    // Header section
    const header = new Box();
    header.addChild(new Text("Modular Agent CLI"));
    header.addChild(
      new Text("Workflow Automation & Agent Management Platform"),
    );

    // Main menu
    this.menuList = new SelectList([
      {
        value: "workflow",
        label: "📋 Workflow Management",
        description: "Manage workflows",
      },
      {
        value: "agent",
        label: "🤖 Agent Loop",
        description: "Run and monitor agents",
      },
      {
        value: "thread",
        label: "🧵 Thread Execution",
        description: "Execute workflows",
      },
      {
        value: "checkpoint",
        label: "💾 Checkpoints",
        description: "Manage checkpoints",
      },
      {
        value: "settings",
        label: "⚙️  Settings",
        description: "Configure CLI",
      },
    ]);
    this.menuList.onSelect = item => {
      if (this.onNavigate) {
        this.onNavigate(item.value);
      }
    };

    // Quick status panel
    this.statusPanel = new Box();
    this.updateStatusPanel();

    // Keyboard shortcuts help
    const helpBox = new Box();
    helpBox.addChild(new Text("↑/↓     - Navigate menu"));
    helpBox.addChild(new Text("Enter   - Select option"));
    helpBox.addChild(new Text("Ctrl+Q  - Quit application"));
    helpBox.addChild(new Text("F1      - Help"));

    this.container.addChild(header);
    this.container.addChild(this.menuList);
    this.container.addChild(this.statusPanel);
    this.container.addChild(helpBox);
  }

  /**
   * Screen message handler - called by TUI dispatcher.
   * Tracks active agents and running threads for dashboard status.
   */
  onMessage(message: BaseComponentMessage): void {
    if (message.category === MessageCategory.AGENT) {
      if (message.type === AgentMessageType.AGENT_START) {
        this.activeAgents++;
      } else if (message.type === AgentMessageType.AGENT_END) {
        this.activeAgents--;
      }
      this.updateStatusPanel();
    } else if (message.category === MessageCategory.WORKFLOW_EXECUTION) {
      if (message.type === WorkflowExecutionMessageType.EXECUTION_START) {
        this.runningThreads++;
      } else if (message.type === WorkflowExecutionMessageType.EXECUTION_END) {
        this.runningThreads--;
      }
      this.updateStatusPanel();
    }
  }

  /**
   * Update status panel with current counts
   */
  private updateStatusPanel() {
    this.statusPanel.clear();
    this.statusPanel.addChild(new Text(`Active Agents: ${this.activeAgents}`));
    this.statusPanel.addChild(new Text(`Running Threads: ${this.runningThreads}`));
    this.statusPanel.addChild(new Text(`Last Updated: ${new Date().toLocaleTimeString()}`));
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    // Delegate to menu list first
    if (this.menuList.handleInput) {
      this.menuList.handleInput(data);
      return true;
    }
    return false;
  }

  destroy(): void {
    // No-op: subscriptions are managed by the TUI app
  }
}
