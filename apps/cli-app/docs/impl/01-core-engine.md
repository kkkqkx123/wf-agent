# Core Engine

## Overview

The core engine provides the foundation for the TUI system, including the component model, rendering pipeline, input handling, terminal management, and event scheduling.

## Component Model

### Interface: `Component` (`src/tui/core/tui.ts:18-23`)

```typescript
interface Component {
  render(width: number): string[];
  handleInput?(data: string): void;
  wantsKeyRelease?: boolean;
  invalidate(): void;
}
```

- `render(width)` returns string lines for the given terminal width
- `handleInput(data)` processes keyboard input
- `invalidate()` clears cached render state

### Interface: `Focusable` (`src/tui/core/tui.ts:25-28`)

```typescript
interface Focusable {
  focused: boolean;
}
```

Components that can receive focus (e.g., Input, Editor).

### Class: `Container` (`src/tui/core/tui.ts:90-124`)

Composite component that holds children and renders them vertically.

- `addChild(component)` / `removeChild(component)` / `clear()`
- `render(width)` concatenates all children's rendered lines
- `invalidate()` propagates to all children

## TUI Engine

### Class: `TUI` (`src/tui/core/tui.ts:126-722`)

Extends `Container`. Central orchestrator managing:

- **Component tree** via `children` (inherited)
- **Overlay stack** (`overlayStack[]`) for modals/dialogs
- **Modal stack** (`modalStack[]`) for modal lifecycle
- **Focus management** (`focusedComponent`)
- **Render scheduling** (16ms min interval, coalesced)
- **Phase manager** (`PhaseManager`) for interaction phases
- **Event scheduler** (`EventScheduler`) for tick sources

#### Input Routing (`handleInput`, line 428-470)

```
stdin data
    ↓
PhaseManager.handleInput() — buffers printable input during Streaming
    ↓
Active Modal (capturesAllKeys) → handleKey()
    ↓
App-level onInput (screen routing, global keybindings)
    ↓
Non-capturing Modal → handleKey()
    ↓
Focused Component → handleInput()
```

#### Overlay System (`showOverlay`, line 254-316)

Overlays are components displayed on top of the main content with:
- Anchor positioning (9 anchor points)
- Margin support
- Visibility functions
- Focus capture control

#### Modal System (`showModal` / `closeModal`, line 474-513)

Modals are overlays with lifecycle management:
- `showModal(modal, options)` opens and returns `ModalHandle`
- `closeModal(modal)` removes from stack, triggers `onClose`
- Auto-cleanup when phase transitions to Idle

#### Rendering Pipeline (`doRender`, line 705-721)

1. Render main content: `this.render(width)`
2. Composite overlays on top: `compositeOverlays()`
3. Delegate to renderer: `renderer.render(lines, width, height)`

## Rendering

### Interface: `Renderer` (`src/tui/core/renderer.ts`)

```typescript
interface Renderer {
  render(lines: string[], width: number, height: number): void;
  reset(): void;
  clearScreen(): void;
  flush(): void;
  shutdown(): void;
  beginSync(): void;
  endSync(): void;
}
```

### Class: `RetainedRenderer` (`src/tui/core/retained-renderer.ts`)

Differential rendering for TTY terminals:
- Tracks previous frame's lines
- Computes minimal diff
- Only redraws changed regions
- Reports `fullRedraws` stat

### Class: `PlainRenderer` (`src/tui/core/plain-renderer.ts`)

Simple pass-through for non-TTY/pipe output.

## Terminal Management

### Interface: `Terminal` (`src/tui/core/terminal.ts:15-55`)

```typescript
interface Terminal {
  start(onInput: (data: string) => void, onResize: () => void): void;
  stop(): void;
  drainInput(maxMs?: number, idleMs?: number): Promise<void>;
  write(data: string): void;
  get columns(): number;
  get rows(): number;
  moveBy(lines: number): void;
  hideCursor(): void;
  showCursor(): void;
  clearLine(): void;
  clearFromCursor(): void;
  clearScreen(): void;
  setTitle(title: string): void;
  setProgress(active: boolean): void;
  get capabilities(): TerminalCapabilities;
}
```

### Class: `ProcessTerminal` (`src/tui/core/terminal.ts:60+`)

Real terminal implementation using `process.stdin/stdout`:
- Raw mode management
- Kitty keyboard protocol support
- Bracketed paste mode (`\x1b[?2004h`)
- Resize coalescing
- OSC 9;4 progress indicators
- Terminal capability detection (`detectCapabilities()`)

## Keyboard Input

### Key Constants (`src/tui/core/keys/`)

- `constants.ts` — Key ID definitions
- `types.ts` — Key type definitions
- `parsing.ts` — Key sequence parsing
- `matching.ts` — Key matching logic
- `kitty-protocol.ts` — Kitty keyboard protocol (CSI-u)
- `legacy-sequences.ts` — Legacy terminal escape sequences

### Keybindings System (`src/tui/core/keybindings.ts`)

#### Input Contexts

```typescript
type InputContext = "global" | "chat" | "selectList" | "modal";
```

Keybindings are context-aware; a binding only matches when its context matches the current TUI context.

#### Keybinding Categories

| Category | Bindings | Context |
|----------|----------|---------|
| Editor Navigation | cursorUp/Down/Left/Right, cursorWordLeft/Right, cursorLineStart/End, pageUp/Down | chat |
| Editor Editing | deleteCharBack/Forward, deleteWordBack/Forward, deleteToLineStart/End, yank, undo | chat |
| Input Actions | newLine, submit, tab, copy | chat/global |
| Selection | up/down/pageUp/pageDown, confirm, cancel | selectList |
| Navigation | up/down, halfPageUp/Down, top/bottom | chat (Normal mode) |
| Global | redraw (Ctrl+L), cancel (Esc) | global |

#### Class: `KeybindingsManager` (line 226-309)

- Stores keybinding definitions and user overrides
- `matches(data, keybinding, context?)` checks if input matches
- Supports user customization via `~/.config/modular-agent/keybindings.json`
- Conflict detection for overlapping bindings

#### Global Singleton

```typescript
function getKeybindings(): KeybindingsManager;
function loadUserKeybindings(): Promise<void>;  // Loads from config file
```

## Interaction Phase System

### Enum: `InteractionPhase` (`src/tui/core/interaction-phase.ts:3-8`)

```typescript
enum InteractionPhase {
  Idle = "idle",
  Streaming = "streaming",
  Approval = "approval",
  Normal = "normal",
}
```

### State Machine (`src/tui/core/interaction-phase.ts:13-18`)

```
Idle → Streaming | Normal
Streaming → Idle | Approval | Normal
Approval → Idle | Streaming
Normal → Idle | Streaming
```

### Class: `PhaseManager` (line 20-87)

- `transition(to, data?)` — validated phase transition
- `handleInput(data)` — buffers printable input during Streaming phase`
- `releaseBufferedInput()` — flushes buffer on phase exit
- `onPhaseChange` callback — updates TUI context/mode
- `onInputRelease` callback — delivers buffered input

#### Phase Behavior

| Phase | Input Behavior | Context | Mode |
|-------|---------------|---------|------|
| Idle | Delegates to focused component | chat | Chat |
| Streaming | Buffers printable chars; Ctrl+C cancels | chat | Chat |
| Approval | Y/Enter approves, N/Esc rejects | modal | Chat |
| Normal | Vim-like navigation (j/k, Ctrl+u/d, g/G) | chat | Normal |

## Event Scheduling

### Class: `EventScheduler` (`src/tui/core/event-scheduler.ts`)

Priority-based tick sources:
- `Animation` (80ms) — spinner animation during Streaming
- `Poll` (500ms) — config file polling
- `Render` (50ms) — periodic re-render

## Utilities

### Text Utilities (`src/tui/core/utils.ts`)

- `visibleWidth(str)` — display width accounting for ANSI codes
- `truncateToWidth(str, max, ellipsis)` — truncate to display width
- `wordWrapLine(str, width)` — word-aware line wrapping
- `wrapTextWithAnsi(str, width)` — wrap preserving ANSI codes
- `isWhitespaceChar(ch)` / `isPunctuationChar(ch)` — character classification
- `getSegmenter()` — Unicode grapheme segmenter
- `applyBackgroundToLine(line, width, bgFn)` — apply background color

### Data Structures

- `UndoStack<T>` (`undo-stack.ts`) — generic undo support
- `KillRing` (`kill-ring.ts`) — Emacs-style kill/yank ring buffer
- `ScrollbackBuffer<T>` (`scrollback.ts`) — ring buffer (max 5000 lines)
- `InputBuffer` (`input-buffer.ts`) — character queue for phase buffering
- `StdinBuffer` (`stdin-buffer.ts`) — splits batched stdin into complete sequences

### Supporting Systems

- `TerminalGuard` (`terminal-guard.ts`) — RAII guard for terminal state restoration on crash
- `ConfigWatcher` (`config-watcher.ts`) — file watcher for hot-reloading config
- `Theme` (`theme.ts`) — color theme system with `ColorRole` definitions
- `Autocomplete` (`autocomplete.ts`) — provider interface + combined provider
- `Fuzzy` (`fuzzy.ts`) — fuzzy matching algorithm for filtering
- `SignalHandler` (`signal-handler.ts`) — SIGINT/SIGTSTP/SIGCONT handling
