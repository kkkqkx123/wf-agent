import { ProcessTerminal, TUI, Container, RetainedRenderer, PlainRenderer } from "./core/index.js";
import type { Renderer } from "./core/renderer.js";
import type { InputContext } from "./core/keybindings.js";
import { getKeybindings, loadUserKeybindings } from "./core/keybindings.js";
import { ConfigWatcher } from "./core/config-watcher.js";
import type { Screen } from "./screens/screen.js";
import { DashboardScreen } from "./screens/dashboard-screen.js";
import { WorkflowScreen } from "./screens/workflow-screen.js";
import { AgentScreen } from "./screens/agent-screen.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";
import { ModalManager } from "./modals/index.js";

const SCREEN_CONTEXTS: Record<string, InputContext> = {
  dashboard: "selectList",
  workflow: "selectList",
  agent: "chat",
};

export class CLIAppTUI {
  private tui: TUI;
  readonly modalManager: ModalManager;
  private terminal: ProcessTerminal;
  private renderer: Renderer;
  private mainContainer: Container;
  private currentScreenId: string = "dashboard";
  private screens: Map<string, Screen> = new Map();
  private isRunning: boolean = false;
  private logger = createContextualLogger({ component: "CLIAppTUI" });
  private configWatcher = new ConfigWatcher();

  constructor(renderer?: Renderer) {
    this.terminal = new ProcessTerminal();
    this.renderer = renderer ?? this.detectRenderer();
    this.tui = new TUI(this.terminal, this.renderer);
    this.mainContainer = new Container();

    this.modalManager = new ModalManager(this.tui);
    this.initializeScreens();
    this.setupGlobalKeybindings();
    this.setupInputRouting();
    this.setupConfigHotReload();
  }

  private setupConfigHotReload(): void {
    const configDir = process.env["XDG_CONFIG_HOME"]
      ? `${process.env["XDG_CONFIG_HOME"]}/modular-agent`
      : `${process.env["HOME"] || process.env["USERPROFILE"] || "."}/.config/modular-agent`;
    const configPath = `${configDir}/keybindings.json`;

    this.configWatcher.watch(configPath, () => {
      try {
        loadUserKeybindings();
        this.logger.info(`Config hot-reloaded`, {}, { path: configPath });
        this.tui.requestRender(true);
      } catch (err) {
        this.logger.error(`Config reload failed`, {}, { path: configPath, error: String(err) });
      }
    });
  }

  private detectRenderer(): Renderer {
    if (!process.stdout.isTTY) return new PlainRenderer();
    if (this.terminal.capabilities.isWindowsConhost) return new PlainRenderer();
    return new RetainedRenderer(this.terminal);
  }

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

  private setupGlobalKeybindings() {
    // Global keybindings are handled via onInput routing (setupInputRouting)
  }

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
      }

      // Ctrl+D — quit when no active input or in global context
      if (kb.matches(data, "tui.editor.deleteCharForward", "global")) {
        // Check if current screen is AgentScreen with non-empty input
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

  public getCurrentScreenId(): string {
    return this.currentScreenId;
  }

  public quit(): void {
    this.stop();
    setTimeout(() => process.exit(0), 100);
  }
}
