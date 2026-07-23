// Core TUI interfaces and classes

// Components
export { Box } from "../components/box.js";
export { Editor, type EditorOptions, type EditorTheme } from "../components/editor.js";
export { FoldableSection, type FoldableSectionOptions } from "../components/foldable-section.js";
export { Input } from "../components/input.js";
export {
  type LoaderIndicatorOptions,
  Loader,
} from "../components/loader.js";
export {
  defaultSelectListTheme,
  type SelectItem,
  SelectList,
  type SelectListLayoutOptions,
  type SelectListTheme,
  type SelectListTruncatePrimaryContext,
} from "../components/select-list.js";
export { Spacer } from "../components/spacer.js";
export { Text } from "../components/text.js";

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
  type InputContext,
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
} from "./keys/index.js";

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
  InputMode,
  isFocusable,
  type OverlayAnchor,
  type OverlayHandle,
  type OverlayMargin,
  type OverlayOptions,
  type SizeValue,
  TUI,
} from "./tui.js";

// Utilities
export {
  isPunctuationChar,
  isWhitespaceChar,
  truncateToWidth,
  type TextChunk,
  visibleWidth,
  wordWrapLine,
  wrapTextWithAnsi,
} from "./utils.js";
