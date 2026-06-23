/**
 * Dashboard screen - main menu and status overview
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";

export class DashboardScreen implements Screen {
  private container: Container;
  private menuList!: SelectList;
  private statusPanel!: Box;
  private onNavigate?: (screenId: string) => void;

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
    this.statusPanel.addChild(new Text("Status: Idle"));

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