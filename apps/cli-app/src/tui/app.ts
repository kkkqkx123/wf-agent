/**
 * Main TUI Application for CLI-App
 * Manages screens, navigation, and global keyboard shortcuts
 */

import { ProcessTerminal, TUI, Container } from "./core/index.js";
import type { InputContext } from "./core/keybindings.js";
import { getKeybindings, loadUserKeybindings } from "./core/keybindings.js";
import type { Screen } from "./screens/screen.js";
import { DashboardScreen } from "./screens/dashboard-screen.js";
import { WorkflowScreen } from "./screens/workflow-screen.js";
import { AgentScreen } from "./screens/agent-screen.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

/**
 * Map screen IDs to their input contexts.
 */
const SCREEN_CONTEXTS: Record<string, InputContext> = {
  dashboard: "selectList",
  workflow: "selectList",
  agent: "chat",
};

export class CLIAppTUI {
  private tui: TUI;
  private terminal: ProcessTerminal;
  private mainContainer: Container;
  private currentScreenId: string = "dashboard";
  private screens: Map<string, Screen> = new Map();
  private isRunning: boolean = false;
  private logger = createContextualLogger({ component: "CLIAppTUI" });

  constructor() {
    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);
    this.mainContainer = new Container();

    this.initializeScreens();
    this.setupGlobalKeybindings();
    this.setupInputRouting();
  }

  /**
   * Initialize all available screens
   */
  private initializeScreens() {
    const dashboardScreen = new DashboardScreen((screenId) => {
      this.showScreen(screenId);
    });
    this.screens.set("dashboard", dashboardScreen);

    const workflowScreen = new WorkflowScreen(() => {
      this.showScreen("dashboard");
    }, this.tui);
    this.screens.set("workflow", workflowScreen);

    const agentScreen = new AgentScreen(() => {
      this.showScreen("dashboard");
    }, this.tui);
    this.screens.set("agent", agentScreen);
  }

  /**
   * Setup global keyboard shortcuts
   */
  private setupGlobalKeybindings() {
    // Global keybindings are handled via onInput routing (setupInputRouting)
  }

  /**
   * Wire up screen-level input routing through TUI context system.
   */
  private setupInputRouting() {
    this.tui.onInput = (data: string, _context: InputContext): boolean => {
      const kb = getKeybindings();
      const currentScreen = this.screens.get(this.currentScreenId);

      // Ctrl+L — force full redraw (fix terminal artifacts / screen corruption)
      if (kb.matches(data, "tui.redraw")) {
        this.tui.requestRender(true);
        return true;
      }

      // Esc — universal cancel: close overlay if open, otherwise pass through
      if (kb.matches(data, "tui.global.cancel")) {
        if (this.tui.hasOverlay()) {
          this.tui.hideOverlay();
          return true;
        }
        // Fall through: let screen handle Esc (close modal → cancel → Normal)
      }

      // Ctrl+D — quit when no active input or in global context
      if (kb.matches(data, "tui.editor.deleteCharForward", "global")) {
        // Check if current screen is AgentScreen with non-empty input
        // If so, let it handle as normal delete-char-forward instead
        if (currentScreen instanceof AgentScreen) {
          const input = (currentScreen as unknown as { messageInput?: { getValue?: () => string } }).messageInput;
          if (input?.getValue?.() !== "") {
            return false; // Delegate to screen (delete char forward)
          }
        }
        this.quit();
        return true;
      }

      // Delegate to current screen's handleInput
      if (currentScreen?.handleInput) {
        return currentScreen.handleInput(data) || false;
      }

      return false;
    };
  }

  /**
   * Start the TUI application
   */
  public async start(): Promise<void> {
    if (this.isRunning) {
      return;
    }

    this.isRunning = true;

    // Load user custom keybindings from config file
    await loadUserKeybindings();

    this.tui.addChild(this.mainContainer);

    this.showScreen("dashboard");
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

    const currentScreen = this.screens.get(this.currentScreenId);
    if (currentScreen?.destroy) {
      currentScreen.destroy();
    }

    this.tui.stop();
  }

  /**
   * Switch to a different screen
   */
  public showScreen(screenId: string): void {
    const screen = this.screens.get(screenId);
    if (!screen) {
      this.logger.warn(`Screen not found`, {}, { screenId });
      return;
    }

    const oldScreen = this.screens.get(this.currentScreenId);
    if (oldScreen?.onDeactivate) {
      oldScreen.onDeactivate();
    }

    this.mainContainer.clear();

    this.currentScreenId = screenId;

    // Set TUI context based on screen
    const context = SCREEN_CONTEXTS[screenId] ?? "global";
    this.tui.setContext(context);

    if (screen.onActivate) {
      screen.onActivate();
    }

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
   * Quit the application
   */
  public quit(): void {
    this.stop();
    setTimeout(() => process.exit(0), 100);
  }
}