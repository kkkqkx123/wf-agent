# TUI Core Modules

## Module Overview

| File | Purpose |
|------|---------|
| `autocomplete.ts` | Autocomplete provider for commands and file paths |
| `editor-component.ts` | Interface definition for custom editor components |
| `fuzzy.ts` | Fuzzy matching algorithms |
| `index.ts` | Module exports entry point |
| `keybindings.ts` | Keyboard binding manager |
| `keys.ts` | Keyboard input parsing (legacy + Kitty protocol) |
| `kill-ring.ts` | Emacs-style kill/yank ring buffer |
| `stdin-buffer.ts` | Stdin buffering for partial sequence handling |
| `terminal-image.ts` | Terminal image rendering (Kitty/iTerm2) |
| `terminal.ts` | Terminal interface and ProcessTerminal implementation |
| `tui.ts` | Core TUI class with differential rendering |
| `undo-stack.ts` | Generic undo stack |
| `utils.ts` | Text utilities (width, wrapping, truncation) |

## Module Details

### autocomplete.ts
Provides autocomplete suggestions for slash commands and file paths.

**Key Classes/Functions:**
- `CombinedAutocompleteProvider` - Handles both commands and file path completion
- `SlashCommand` - Interface for commands with optional argument completion
- `AutocompleteProvider` - Core interface for providing suggestions

**Features:**
- Fuzzy file search using `fd` (fast, respects .gitignore)
- Path prefix parsing (@, quoted paths, ~/ expansion)
- Argument completion for slash commands
- Quote handling and proper cursor positioning

### editor-component.ts
Interface for custom editor implementations.

**Interface: `EditorComponent`**
- Required: `getText()`, `setText()`, `handleInput()`, `onSubmit?`, `onChange?`
- Optional: `addToHistory?`, `insertTextAtCursor?`, `getExpandedText?`, `setAutocompleteProvider?`

### fuzzy.ts
Fuzzy string matching with scoring.

**Functions:**
- `fuzzyMatch(query, text)` - Returns `{ matches, score }` (lower = better)
- `fuzzyFilter(items, query, getText)` - Filters and sorts items by match quality
- Supports space-separated tokens (all must match)

**Scoring factors:**
- Word boundary matches (bonus)
- Consecutive matches (reward)
- Gap penalty (cost increases with distance)
- Alphanumeric swap detection (e.g., "ts10" matches "10ts")

### index.ts
Re-exports all public APIs from the module.

### keybindings.ts
Manages keyboard bindings with user configuration support.

**Key Classes:**
- `KeybindingsManager` - Manages binding resolution and conflicts
- `TUI_KEYBINDINGS` - Default keybinding definitions

**Features:**
- User binding overrides via `KeybindingsConfig`
- Conflict detection
- Default bindings for editor, input, and selection actions

### keys.ts
Keyboard input parsing supporting both legacy and Kitty keyboard protocols.

**Key Functions:**
- `matchesKey(data, keyId)` - Check if input matches a key identifier
- `parseKey(data)` - Parse input and return key identifier
- `decodeKittyPrintable(data)` - Extract printable char from CSI-u sequences
- `setKittyProtocolActive(active)` - Toggle Kitty protocol state

**Supported Key Identifiers:**
- Single keys: escape, tab, enter, backspace, delete, home, end, space, arrows
- Modifiers: ctrl, shift, alt, super
- Combined: ctrl+c, shift+enter, ctrl+alt+x, etc.

**Protocol Support:**
- Kitty CSI-u sequences (with flags 1, 2, 4)
- xterm modifyOtherKeys
- Legacy terminal sequences
- Bracketed paste mode

### kill-ring.ts
Ring buffer for Emacs-style kill/yank operations.

**Class: `KillRing`**
- `push(text, { prepend, accumulate })` - Add text
- `peek()` - Get most recent without removing
- `rotate()` - Cycle through entries (for yank-pop)

**Behavior:**
- Consecutive kills can accumulate
- Backward deletion prepends, forward deletion appends

### stdin-buffer.ts
Buffers stdin data to emit complete escape sequences.

**Class: `StdinBuffer` extends EventEmitter**
- Events: `data` (single sequence), `paste` (bracketed paste content)

**Handles:**
- Partial CSI sequences arriving across chunks
- Bracketed paste mode (\x1b[200~ ... \x1b[201~)
- Sequence types: CSI, OSC, DCS, APC, SS3, meta keys

### terminal-image.ts
Terminal image rendering using Kitty IAL and iTerm2 protocols.

**Key Functions:**
- `detectCapabilities()` - Detect terminal features
- `renderImage(base64Data, dims, options)` - Render image, returns sequence
- `encodeKitty()` - Encode for Kitty protocol
- `encodeITerm2()` - Encode for iTerm2 protocol
- `getImageDimensions(base64Data, mimeType)` - Extract dimensions from PNG/JPEG/GIF/WebP
- `hyperlink(text, url)` - OSC 8 hyperlink

**Capabilities:**
- Images: "kitty", "iterm2", or null
- True color support
- Hyperlink support
- Detects: Kitty, Ghostty, WezTerm, iTerm2, VSCode, Alacritty

### terminal.ts
Terminal interface and process-based implementation.

**Interface: `Terminal`**
- Input/output management
- Cursor control
- Screen clearing
- Title and progress indicators

**Class: `ProcessTerminal`**
- Raw mode stdin
- Bracketed paste mode
- Kitty keyboard protocol negotiation
- Windows VT input support (via koffi)

**Features:**
- Query/enable Kitty protocol
- Enable xterm modifyOtherKeys as fallback
- Write logging (PI_TUI_WRITE_LOG)
- Input draining before exit

### tui.ts
Core TUI implementation with differential rendering.

**Classes:**
- `Component` - Base interface for all UI elements
- `Container` - Component that contains children
- `TUI` - Main rendering engine

**Key TUI Features:**
- **Differential rendering**: Only updates changed lines
- **Overlay system**: Modal components with positioning
- **Focus management**: Focusable component tracking
- **Cursor positioning**: Hardware cursor for IME support
- **Synchronized output**: Uses \x1b[?2026h/l for flicker-free rendering

**Overlay Options:**
- Anchoring: center, top-left, bottom-right, etc.
- Sizing: width, maxHeight (absolute or percentage)
- Margins from edges
- Visibility callback

**Rendering Pipeline:**
1. Render all components
2. Composite overlays
3. Extract cursor position
4. Apply line resets
5. Differential update

### undo-stack.ts
Generic undo stack with clone-on-push semantics.

**Class: `UndoStack<S>`**
- `push(state)` - Deep clone and push
- `pop()` - Return most recent
- `clear()` - Remove all
- `length` - Stack size

### utils.ts
Text processing utilities for terminal rendering.

**Key Functions:**

**Width calculation:**
- `visibleWidth(str)` - Terminal column width (accounts for ANSI, wide chars, emoji)
- `graphemeWidth(segment)` - Single grapheme width
- Uses Intl.Segmenter for proper grapheme cluster handling

**Text wrapping:**
- `wrapTextWithAnsi(text, width)` - Word wrap preserving ANSI codes
- `breakLongWord(word, width, tracker)` - Break long words character by character

**Truncation:**
- `truncateToWidth(text, maxWidth, ellipsis, pad)` - Truncate with optional padding
- `truncateFragmentToWidth()` - Internal truncation helper

**Line extraction:**
- `sliceByColumn(line, start, length)` - Extract visible columns
- `extractSegments()` - Extract before/after regions for overlay compositing

**Background:**
- `applyBackgroundToLine(line, width, bgFn)` - Apply background and pad

**Character classification:**
- `isWhitespaceChar(char)` - Check for whitespace
- `isPunctuationChar(char)` - Check for punctuation

**ANSI handling:**
- `extractAnsiCode(str, pos)` - Parse ANSI escape sequences
- `AnsiCodeTracker` - Track active SGR codes for style preservation

**Constants:**
- `CURSOR_MARKER = "\x1b_pi:c\x07"` - Zero-width cursor position marker