# TUI Components

Terminal UI component library for rendering interactive text-based interfaces.

## Components Overview

| Component | File | Purpose |
|-----------|------|---------|
| Box | `box.ts` | Container component with padding and background |
| CancellableLoader | `cancellable-loader.ts` | Loading indicator with Escape cancellation |
| Editor | `editor.ts` | Multi-line text editor with full editing capabilities |
| Image | `image.ts` | Terminal image rendering (Kitty protocol) |
| Input | `input.ts` | Single-line text input field |
| Loader | `loader.ts` | Animated loading spinner |
| Markdown | `markdown.ts` | Markdown content renderer |
| SelectList | `select-list.ts` | Selectable list with filtering |
| SettingsList | `settings-list.ts` | Settings panel with submenu support |
| Spacer | `spacer.ts` | Empty vertical spacing |
| Text | `text.ts` | Multi-line text with word wrapping |
| TruncatedText | `truncated-text.ts` | Single-line truncated text |

## Component Details

### Box
Container that wraps children with configurable padding and optional background color.
- **Features**: Padding control (X/Y), background function, cache optimization
- **Usage**: Wrapper for grouped UI elements

### CancellableLoader
Extends Loader with abort capability via Escape key.
- **Features**: AbortSignal, onAbort callback
- **Usage**: Async operations that can be cancelled by user

### Editor
Full-featured multi-line text editor with extensive keybindings.
- **Features**:
  - Cursor movement (arrows, Home/End, Ctrl+arrows)
  - Text selection and deletion (words, lines)
  - Undo/Redo with coalescing
  - Kill ring (Emacs-style yank/yank-pop)
  - Bracketed paste support
  - Large paste handling with markers
  - Word wrap with Intl.Segmenter
  - Autocomplete integration (slash commands, @/# mentions)
  - History navigation (up/down arrows)
  - Character jump mode
  - Sticky column for vertical movement

### Image
Renders images in terminal using Kitty IAL protocol.
- **Features**: Fallback for unsupported terminals, dimension caching
- **Usage**: Inline image display in chat/UI

### Input
Single-line text input with basic editing.
- **Features**: Cursor movement, deletion, kill ring, undo, horizontal scrolling
- **Usage**: Simple text prompts

### Loader
Animated loading indicator.
- **Features**: Customizable frames, interval timing, message display
- **Usage**: Loading states

### Markdown
Markdown renderer with ANSI terminal styling.
- **Features**:
  - Headings (with underline for H1)
  - Bold, italic, strikethrough, underline
  - Code blocks with syntax highlighting
  - Blockquotes with border
  - Lists (ordered/unordered, nested)
  - Tables with width-aware rendering
  - Links with OSC 8 hyperlinks
  - Horizontal rules
  - Image line detection

### SelectList
Keyboard-navigable selection list.
- **Features**:
  - Up/Down navigation with wrap
  - Filtering by prefix match
  - Scroll indicators
  - Two-column layout (label + description)
  - Custom truncation

### SettingsList
Interactive settings panel.
- **Features**:
  - Value cycling (Enter/Space)
  - Submenu support
  - Fuzzy search
  - Two-column layout (label + value)
  - Description display
  - Cursor indicator

### Spacer
Vertical whitespace component.
- **Features**: Configurable line count
- **Usage**: Layout spacing

### Text
Multi-line text display with word wrapping.
- **Features**: Padding, background color, cache optimization
- **Usage**: Static text display

### TruncatedText
Single-line text that truncates to fit width.
- **Features**: ANSI-aware truncation, padding
- **Usage**: Labels, headers, short messages