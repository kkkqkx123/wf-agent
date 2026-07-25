/**
 * Dashboard screen - main menu
 *
 * Flat list navigation: arrow keys to navigate, Enter to select.
 * No panels, no status cards, no permanent help bar.
 */

import { Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";

export class DashboardScreen implements Screen {
  private container: Container;
  private menuList!: SelectList;
  private onNavigate?: (screenId: string) => void;

  constructor(onNavigate?: (screenId: string) => void) {
    this.onNavigate = onNavigate;
    this.container = new Container();
    this.setupLayout();
  }

  onActivate(): void {
    // No data to refresh — menu is static
  }

  private setupLayout() {
    this.menuList = new SelectList([
      { value: "workflow", label: "Workflows", description: "Manage workflows" },
      { value: "agent", label: "Agent Loops", description: "Run and monitor agents" },
      { value: "thread", label: "Threads", description: "Execute workflows" },
      { value: "checkpoint", label: "Checkpoints", description: "Manage checkpoints" },
      { value: "settings", label: "Settings", description: "Configure CLI" },
    ]);
    this.menuList.onSelect = item => {
      this.onNavigate?.(item.value);
    };

    const statusBar = new Text("↑/↓ navigate  Enter select  ? help  Ctrl+Q quit", 0, 0);

    this.container.addChild(this.menuList);
    this.container.addChild(statusBar);
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    return this.menuList.handleInput?.(data), true;
  }

  destroy(): void {}
}
