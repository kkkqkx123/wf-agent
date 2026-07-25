# UI Components

## Design Rules

- Default padding is 0,0 (flat layout)
- No redundant section headers inside panels
- Box only used for modals/overlays, not screen sections

## Box

### `Box` (`src/tui/components/box.ts`)

Container that applies padding and background to children. **Default padding is 0,0** (changed from 1,1).

```typescript
constructor(paddingX = 0, paddingY = 0, bgFn?: (text: string) => string)
```

Use non-zero padding only for modals and overlays that need visual separation.

## SelectList

### `SelectList` (`src/tui/components/select-list.ts`)

Primary navigation component for all list screens.

#### Render Format

```
→ item-label      description
  another-item    its description
  (selected)
```

- `→` marks selected item
- Two-column layout when width > 40
- Scroll indicator `(N/M)` when items exceed visible area

#### Input Handling

| Key | Action |
|-----|--------|
| ↑ / ↓ | Navigate (wraps) |
| Enter | Select (calls `onSelect`) |
| Esc | Cancel (calls `onCancel`) |

## Text

### `Text` (`src/tui/components/text.ts`)

Single or multi-line text with optional padding.

```typescript
constructor(text?: string, paddingX = 1, paddingY = 1, customBgFn?)
```

For status bars and flat layouts, use `new Text(content, 0, 0)` to avoid padding.

## Input

### `Input` (`src/tui/components/input.ts`)

Single-line text input with editing capabilities.

- Kill ring (copy/paste)
- Undo stack
- Bracketed paste support
- `onSubmit` callback for Enter key

## IterationPanel

### `IterationPanel` (`src/tui/components/iteration-panel.ts`)

Displays iteration progress. **No section header** — renders only data lines.

#### Render Format

```
▶ Iteration 1: 3 tools, 5s →
✓ Iteration 2: 5 tools, 8s ↑
✓ Iteration 3: 2 tools, 3s ↓
```

Status icons: ▶ running, ✓ completed, ✗ error
Trend arrows: ↑ slower, ↓ faster, → same (vs previous iteration)

#### Options

```typescript
interface IterationPanelOptions {
  maxHeight?: number;  // default: 10
}
```

## ToolCallIndicator

### `ToolCallIndicator` (`src/tui/components/tool-call-indicator.ts`)

Displays active and completed tool calls. **No section headers**.

#### Render Format

```
🔄 search (3s)
▶ ✓ read_file (120ms)
▼ ✓ write_file (45ms)
    Args: { "path": "...", "content": "..." }
```

- Active calls: `🔄 name (elapsed)s`
- Completed: `▶/▼ name (duration)ms`
- `▶` collapsed, `▼` expanded (shows Args)

#### Options

```typescript
interface ToolCallIndicatorOptions {
  maxDisplayCalls?: number;  // default: 5
}
```

## FoldableSection (Deprecated)

### `FoldableSection` (`src/tui/components/foldable-section.ts`)

**Status: Deprecated.** This component is no longer used in any screen.

Previously provided collapsible sections with `[+]` / `[-]` markers. Replaced by drill-down navigation — content that was hidden behind a fold now gets its own detail view.
