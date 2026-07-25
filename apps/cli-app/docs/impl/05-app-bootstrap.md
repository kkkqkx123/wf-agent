# Application Bootstrap & Control Logic

## Overview

The application entry point and bootstrap process that wires together the CLI command system and TUI mode.

## Entry Point

### Binary Entry (`scripts/modular-agent.js`)

ESM import wrapper that loads the compiled `dist/index.ts`.

### Main Entry (`src/index.ts`)

Orchestrates:
1. TOML parser initialization
2. Configuration loading (with env variable overrides)
3. Output system initialization
4. Formatter initialization
5. Logger initialization
6. SDK bootstrap (`createAppSDK()` + `StorageManager`)
7. User interaction handler setup
8. Dependency container initialization

## Application Class

### `CLIAppTUI` (`src/tui/app.ts`)

Main TUI application class managing the screen system and global keybindings.

#### Constructor

```typescript
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
```

#### Renderer Detection

```typescript
private detectRenderer(): Renderer {
  if (!process.stdout.isTTY) return new PlainRenderer();
  if (this.terminal.capabilities.isWindowsConhost) return new PlainRenderer();
  return new RetainedRenderer(this.terminal);
}
```

#### Screen Navigation

```typescript
public showScreen(screenId: string): void {
  // Deactivate old screen
  oldScreen.onDeactivate?.();
  // Clear container
  this.mainContainer.clear();
  // Update current screen ID
  this.currentScreenId = screenId;
  // Set input context based on screen
  this.tui.setContext(SCREEN_CONTEXTS[screenId] ?? "global");
  // Activate new screen
  screen.onActivate?.();
  // Add to container
  this.mainContainer.addChild(screen.render());
  // Request render
  this.tui.requestRender();
}
```

Screen-to-context mapping:
- `dashboard` → `selectList`
- `workflow` → `selectList`
- `agent` → `chat`

#### Global Input Routing (`setupInputRouting`)

```typescript
this.tui.onInput = (data, context) => {
  // Ctrl+L — force full redraw
  if (kb.matches(data, "tui.redraw")) { ... }

  // Esc — close overlay if open
  if (kb.matches(data, "tui.global.cancel")) { ... }

  // Ctrl+D — quit (unless AgentScreen has input)
  if (kb.matches(data, "tui.editor.deleteCharForward", "global")) { ... }

  // Delegate to current screen
  currentScreen?.handleInput(data);
};
```

#### Config Hot Reload

```typescript
private setupConfigHotReload(): void {
  this.configWatcher.watch(configPath, () => {
    loadUserKeybindings();
    this.tui.requestRender(true);
  });
}
```

Watches `~/.config/modular-agent/keybindings.json` for changes.

#### Lifecycle

```typescript
public async start(): Promise<void> {
  await loadUserKeybindings();
  this.tui.addChild(this.mainContainer);
  this.showScreen("dashboard");
  this.tui.start();
}

public stop(): void {
  currentScreen?.destroy();
  this.tui.stop();
}

public quit(): void {
  this.stop();
  setTimeout(() => process.exit(0), 100);
}
```

## Dependency Injection

### `CLIDependencyContainer` (`src/services/container.ts`)

Manages service lifecycle:
- SDK instance
- `TerminalManager`
- `ExecutionService`
- `WorkflowExecutionAdapter`
- `CLIUserInteractionHandler`

Global singleton pattern: `initializeContainer()` / `getContainer()`

### `sdk-globals.ts` (`src/services/sdk-globals.ts`)

Global SDK instance holder:
- `getSDKInstance()` / `setSDKInstance()`

## Mode Detection

TUI mode is activated when:
- `--tui` flag is provided, OR
- `executionMode === "interactive"` in config AND no arguments provided

Requirements:
- Must have TTY stdout
- Must use text output format

Otherwise falls back to CLI help.

## Commander Hooks

- `preAction` — runs bootstrap before any command
- `postAction` — calls `shutdown()` after every command; exits cleanly in headless mode

## Configuration System

### Config Types (`src/config/cli/types.ts`)

`CLIConfig` extends `DefaultAppConfigSchema` with CLI-specific fields.

### Config Loader (`src/config/cli/loader.ts`)

- TOML/JSON config file loading
- Environment variable overrides via `CLI_ENV_MAPPING`

### Config Accessor (`src/config/cli/accessor.ts`)

Singleton `CLIConfigAccessor` for runtime config access.

### Default Config (`src/config/cli/defaults.ts`)

`DEFAULT_CONFIG` with sensible defaults.

## Output System

### `CLIOutput` (`src/utils/output.ts`)

Global output singleton supporting:
- Text format
- JSON format
- Table format

### Output Router (`src/utils/output-router.ts`)

Routes output based on configured format.

### Formatters (`src/utils/formatters/`)

18 domain-specific formatters:
- `workflow`, `checkpoint`, `llm-profile`, `script`, `tool`, `trigger`
- `message`, `variable`, `event`, `agent-loop`, `plugin`, `task`
- `hook`, `search-formatters`, `storage-formatters`, `progress-formatters`
- `version-formatters`, `comparison-formatters`, `graph-formatters`
- `iteration-formatters`

Each formatter provides `format*` (single) and `format*List` (collection) variants.

## Error Handling

### `CLIError` (`src/types/cli-types.ts`)

CLI-specific error type with:
- Error code
- Context data
- Source error reference

### Error Handler (`src/utils/error-handler.ts`)

Centralized error handling utilities.

### Exit Manager (`src/utils/exit-manager.ts`)

Process exit management with cleanup hooks.
