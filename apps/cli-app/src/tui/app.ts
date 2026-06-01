/**
 * Main TUI Application for CLI-App
 * Manages screens, navigation, and global keyboard shortcuts
 */

import { ProcessTerminal, TUI, Container } from "./core/index.js";
import type { Screen } from "./screens/screen.js";
import { DashboardScreen } from "./screens/dashboard-screen.js";
import { WorkflowScreen } from "./screens/workflow-screen.js";
import { AgentScreen } from "./screens/agent-screen.js";
import { MessageBus } from "@wf-agent/sdk/api";
import { HumanRelayService, DisplayOutputService } from "../services/io/index.js";
import { TUIHumanRelayHandler } from "./handlers/tui-human-relay-handler.js";
import { FunctionalFileHandler, DisplayFileHandler, TUIOutputHandler } from "../handlers/index.js";
import { CLI_ROUTING_RULES } from "../config/routing-rules.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

export class CLIAppTUI {
  private tui: TUI;
  private terminal: ProcessTerminal;
  private mainContainer: Container;
  private currentScreenId: string = "dashboard";
  private screens: Map<string, Screen> = new Map();
  private isRunning: boolean = false;
  
  // Message bus and file IO services
  private messageBus: MessageBus;
  private humanRelayService: HumanRelayService;
  private displayOutputService: DisplayOutputService;
  private humanRelayHandler: TUIHumanRelayHandler;
  private tuiOutputHandler: TUIOutputHandler;
  private logger = createContextualLogger({ component: "CLIAppTUI" });

  constructor() {
    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);
    this.mainContainer = new Container();

    // Initialize message bus with routing rules
    this.messageBus = new MessageBus(CLI_ROUTING_RULES, {
      maxHistorySize: 1000,
      enableHistory: true,
      asyncHandlers: true,
    });

    // Initialize file IO services
    this.humanRelayService = new HumanRelayService({ baseDir: ".wf-agent/function" });
    this.displayOutputService = new DisplayOutputService({ baseDir: ".wf-agent/display" });

    // Initialize human relay handler with services
    this.humanRelayHandler = new TUIHumanRelayHandler(this.tui, this.humanRelayService);

    // Initialize TUI output handler
    this.tuiOutputHandler = new TUIOutputHandler();

    // Register message handlers
    this.initializeMessageHandlers();

    this.initializeScreens();
    this.setupGlobalKeybindings();
  }

  /**
   * Initialize all available screens
   */
  private initializeScreens() {
    // Register dashboard screen - pass messageBus for TUI-targeted message subscription
    const dashboardScreen = new DashboardScreen(this.messageBus, (screenId) => {
      this.showScreen(screenId);
    });
    this.screens.set("dashboard", dashboardScreen);

    // Register workflow screen - pass messageBus for node execution events
    const workflowScreen = new WorkflowScreen(this.messageBus, () => {
      this.showScreen("dashboard");
    });
    this.screens.set("workflow", workflowScreen);

    // Register agent screen - pass messageBus for agent lifecycle events
    const agentScreen = new AgentScreen(this.messageBus, () => {
      this.showScreen("dashboard");
    });
    this.screens.set("agent", agentScreen);

    // TODO: Register other screens in future phases
    // this.screens.set("thread", new ThreadScreen());
    // this.screens.set("checkpoint", new CheckpointScreen());
    // this.screens.set("settings", new SettingsScreen());
  }
  
  /**
   * Initialize message handlers
   */
  private initializeMessageHandlers() {
    // TUI message handling: screens can subscribe directly to MessageBus
    // (existing pattern) or use TUIOutputHandler for TUI-targeted messages.
    // The TUIOutputHandler bridges routing rules and screen display.
    this.messageBus.registerHandler(this.tuiOutputHandler);

    // Register functional file handler
    this.messageBus.registerHandler(new FunctionalFileHandler(this.humanRelayService));
  
    // Register display file handler
    this.messageBus.registerHandler(new DisplayFileHandler(this.displayOutputService));
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

    // Flush and close all message handlers
    this.messageBus.flush().catch(err => {
      this.logger.error("Failed to flush handlers during shutdown", { error: err });
    });
    this.messageBus.close().catch(err => {
      this.logger.error("Failed to close handlers during shutdown", { error: err });
    });

    // Stop TUI
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
   * @deprecated Reserved for future use - currently not called
   */
  // @ts-ignore - Reserved for future use
  private __handleGlobalInput(_data: string): boolean {
    // Ctrl+Q to quit
    if (_data === "\x11" || _data === "\u0011") {
      this.stop();
      setTimeout(() => process.exit(0), 100);
      return true;
    }

    // F1 for help (ESC [ P where P is keycode for F1)
    // F1 typically sends ESC OP or ESC [ 1 1 ~
    if (_data === "\x1bOP" || _data === "\x1b[11~") {
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
    this.logger.info("Help requested - to be implemented");
  }

  /**
   * Quit the application
   */
  public quit(): void {
    this.stop();
    setTimeout(() => process.exit(0), 100);
  }

  /**
   * Get message bus instance
   */
  public getMessageBus(): MessageBus {
    return this.messageBus;
  }

  /**
   * Get human relay service
   */
  public getHumanRelayService(): HumanRelayService {
    return this.humanRelayService;
  }

  /**
   * Get display output service
   */
  public getDisplayOutputService(): DisplayOutputService {
    return this.displayOutputService;
  }

  /**
   * Get human relay handler
   */
  public getHumanRelayHandler(): TUIHumanRelayHandler {
    return this.humanRelayHandler;
  }
}
