// Core TUI interfaces and classes

// Autocomplete support
export {
  type AutocompleteItem,
  type AutocompleteProvider,
  type AutocompleteSuggestions,
  CombinedAutocompleteProvider,
  type SlashCommand,
} from "./autocomplete.js";

// Fuzzy matching
export { type FuzzyMatch, fuzzyFilter, fuzzyMatch } from "./fuzzy.js";

// Keybindings
export {
  getKeybindings,
  type Keybinding,
  type KeybindingConflict,
  type KeybindingDefinition,
  type KeybindingDefinitions,
  type Keybindings,
  type KeybindingsConfig,
  KeybindingsManager,
  setKeybindings,
  TUI_KEYBINDINGS,
} from "./keybindings.js";

// Keyboard input handling
export {
  decodeKittyPrintable,
  isKeyRelease,
  isKeyRepeat,
  isKittyProtocolActive,
  Key,
  type KeyEventType,
  type KeyId,
  matchesKey,
  parseKey,
  setKittyProtocolActive,
} from "./keys.js";

// Input buffering for batch splitting
export { StdinBuffer, type StdinBufferEventMap, type StdinBufferOptions } from "./stdin-buffer.js";

// Terminal interface and implementations
export { ProcessTerminal, type Terminal } from "./terminal.js";

// Kill ring for Emacs-style operations
export { KillRing } from "./kill-ring.js";

// Undo stack
export { UndoStack } from "./undo-stack.js";

// Core TUI engine
export {
  type Component,
  Container,
  CURSOR_MARKER,
  type Focusable,
  isFocusable,
  type OverlayAnchor,
  type OverlayHandle,
  type OverlayMargin,
  type OverlayOptions,
  type SizeValue,
  TUI,
} from "./tui.js";

// Utilities
export { truncateToWidth, visibleWidth, wrapTextWithAnsi } from "./utils.js";
