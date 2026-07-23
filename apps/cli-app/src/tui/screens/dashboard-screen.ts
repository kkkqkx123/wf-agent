/**
 * Dashboard screen - main menu and status overview
 *
 * Displays system status summary: active agents, recent executions,
 * and basic resource info.
 */

import os from "node:os";
import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import { AgentLoopAdapter } from "../../adapters/agent-loop-adapter.js";
import { WorkflowExecutionAdapter } from "../../adapters/workflow-execution-adapter.js";

export class DashboardScreen implements Screen {
  private container: Container;
  private menuList!: SelectList;
  private statusPanel!: Box;
  private onNavigate?: (screenId: string) => void;
  private agentAdapter: AgentLoopAdapter;
  private executionAdapter: WorkflowExecutionAdapter;

  /** Cached active agent count (refreshed in onActivate) */
  private activeAgentCount: number = 0;
  /** Cached recent execution summaries (refreshed in onActivate) */
  private recentExecutions: string[] = [];
  /** Cached system resource info (refreshed on each render) */
  private memUsedMB: number = 0;
  private memTotalMB: number = 0;
  private memPercent: string = "?";
  private cpuCores: number = 0;
  private cpuLoad: string = "?";

  constructor(onNavigate?: (screenId: string) => void) {
    this.onNavigate = onNavigate;
    this.agentAdapter = new AgentLoopAdapter();
    this.executionAdapter = new WorkflowExecutionAdapter();
    this.container = new Container();
    this.refreshSystemResources();
    this.setupLayout();
    // Kick off first data fetch
    this.refreshData();
  }

  /**
   * Screen lifecycle: refresh cached data each time the dashboard becomes visible.
   */
  onActivate(): void {
    this.refreshData();
  }

  /**
   * Fetch async data (agents, executions) and cache it for synchronous render.
   */
  private async refreshData(): Promise<void> {
    // Active agents
    try {
      const running = this.agentAdapter.listRunningAgentLoops();
      this.activeAgentCount = running.length;
    } catch {
      this.activeAgentCount = 0;
    }

    // Recent execution records (last 5)
    try {
      const executions = await this.executionAdapter.listWorkflowExecutions();
      const last5 = executions.slice(-5).reverse();
      this.recentExecutions = last5.map((exec) => {
        const e = exec as any;
        const label = e.name || e.executionId || e.id || "unknown";
        const status = e.status || "unknown";
        return `${label} [${status}]`;
      });
    } catch {
      this.recentExecutions = [];
    }
  }

  /**
   * Refresh synchronous system resource info.
   */
  private refreshSystemResources(): void {
    const totalMem = os.totalmem();
    const freeMem = os.freemem();
    const usedMem = totalMem - freeMem;
    this.memTotalMB = Math.round(totalMem / 1024 / 1024);
    this.memUsedMB = Math.round(usedMem / 1024 / 1024);
    this.memPercent = totalMem > 0 ? ((usedMem / totalMem) * 100).toFixed(1) : "?";
    this.cpuCores = os.cpus().length;
    const loadAvg = os.loadavg();
    this.cpuLoad = loadAvg.length > 0 ? loadAvg[0]!.toFixed(1) : "?";
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

    // Quick status panel (content refreshed on render)
    this.statusPanel = new Box();

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
   * Populate the status panel with cached data.
   */
  private updateStatusPanel(): void {
    this.statusPanel.clear();

    // Active agent count
    this.statusPanel.addChild(
      new Text(`Active Agents: ${this.activeAgentCount} running`, 1, 0),
    );

    // Recent execution records (last 5)
    this.statusPanel.addChild(new Text("Recent Executions:", 1, 0));
    if (this.recentExecutions.length === 0) {
      this.statusPanel.addChild(new Text("  (none)", 1, 0));
    } else {
      for (const line of this.recentExecutions) {
        this.statusPanel.addChild(new Text(`  ${line}`, 1, 0));
      }
    }

    // System resources
    this.refreshSystemResources();
    this.statusPanel.addChild(
      new Text(`Memory: ${this.memUsedMB}MB / ${this.memTotalMB}MB (${this.memPercent}%)`, 1, 0),
    );
    this.statusPanel.addChild(
      new Text(`CPU: ${this.cpuCores} cores, Load: ${this.cpuLoad}`, 1, 0),
    );
  }

  render(): Container {
    this.updateStatusPanel();
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
