/**
 * Main TUI Application for CLI-App
 * Manages screens, navigation, and global keyboard shortcuts
 */

import { ProcessTerminal, TUI, Container } from "./core/index.js";
import type { Screen } from "./screens/screen.js";
import { DashboardScreen } from "./screens/dashboard-screen.js";
import { WorkflowScreen } from "./screens/workflow-screen.js";
import { AgentScreen } from "./screens/agent-screen.js";

export class CLIAppTUI {
  private tui: TUI;
  private terminal: ProcessTerminal;
  private mainContainer: Container;
  private currentScreenId: string = "dashboard";
  private screens: Map<string, Screen> = new Map();
  private isRunning: boolean = false;

  constructor() {
    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);
    this.mainContainer = new Container();

    this.initializeScreens();
    this.setupGlobalKeybindings();
  }

  /**
   * Initialize all available screens
   */
  private initializeScreens() {
    // Register dashboard screen
    const dashboardScreen = new DashboardScreen(screenId => {
      this.showScreen(screenId);
    });
    this.screens.set("dashboard", dashboardScreen);

    // Register workflow screen
    const workflowScreen = new WorkflowScreen(() => {
      this.showScreen("dashboard");
    });
    this.screens.set("workflow", workflowScreen);

    // Register agent screen
    const agentScreen = new AgentScreen(() => {
      this.showScreen("dashboard");
    });
    this.screens.set("agent", agentScreen);

    // TODO: Register other screens in future phases
    // this.screens.set("thread", new ThreadScreen());
    // this.screens.set("checkpoint", new CheckpointScreen());
    // this.screens.set("settings", new SettingsScreen());
  }

  /**
   * Setup global keyboard shortcuts
   */
  private setupGlobalKeybindings() {
    // Global shortcuts will be handled via handleInput
    // Ctrl+Q to quit
    // F1 for help (to be implemented)
    // Tab to cycle through panels (future enhancement)
  }

  /**
   * Start the TUI application
   */
  public start(): void {
    if (this.isRunning) {
      return;
    }

    this.isRunning = true;

    // Add main container to TUI
    this.tui.addChild(this.mainContainer);

    // Show initial screen
    this.showScreen("dashboard");

    // Start the TUI event loop
    this.tui.start();
  }

  /**
   * Stop the TUI application
   */
  public stop(): void {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;

    // Cleanup current screen
    const currentScreen = this.screens.get(this.currentScreenId);
    if (currentScreen?.destroy) {
      currentScreen.destroy();
    }

    // Stop TUI
    this.tui.stop();
  }

  /**
   * Switch to a different screen
   */
  public showScreen(screenId: string): void {
    const screen = this.screens.get(screenId);
    if (!screen) {
      console.warn(`Screen "${screenId}" not found`);
      return;
    }

    // Deactivate current screen
    const oldScreen = this.screens.get(this.currentScreenId);
    if (oldScreen?.onDeactivate) {
      oldScreen.onDeactivate();
    }

    // Clear main container
    this.mainContainer.clear();

    // Activate new screen
    this.currentScreenId = screenId;
    if (screen.onActivate) {
      screen.onActivate();
    }

    // Render new screen
    this.mainContainer.addChild(screen.render());
    this.tui.requestRender();
  }

  /**
   * Get current screen ID
   */
  public getCurrentScreenId(): string {
    return this.currentScreenId;
  }

  /**
   * Handle global keyboard input
   */
  private handleGlobalInput(data: string): boolean {
    // Ctrl+Q to quit
    if (data === "\x11" || data === "\u0011") {
      this.stop();
      setTimeout(() => process.exit(0), 100);
      return true;
    }

    // F1 for help (ESC [ P where P is keycode for F1)
    // F1 typically sends ESC OP or ESC [ 1 1 ~
    if (data === "\x1bOP" || data === "\x1b[11~") {
      this.showHelp();
      return true;
    }

    return false;
  }

  /**
   * Show help overlay (to be implemented)
   */
  private showHelp(): void {
    // TODO: Implement help overlay using TUI overlay system
    console.log("Help requested - to be implemented");
  }

  /**
   * Quit the application
   */
  public quit(): void {
    this.stop();
    setTimeout(() => process.exit(0), 100);
  }
}
