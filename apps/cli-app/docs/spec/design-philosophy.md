# TUI Design Philosophy

## Core Principles

### 1. List-Driven Navigation

The terminal is a linear medium. Content flows top-to-bottom. The primary interaction model is:

- **List view**: Navigate with arrow keys, select with Enter
- **Detail view**: Drill into a list item to see its full content
- **Modal**: Temporary overlay for confirmations, pickers, forms

No side-by-side panels. No split layouts. No card grids.

### 2. Zero Chrome

Every line of the terminal should display user data, not UI decoration.

- No Box padding around content
- No section headers inside panels (the view title is sufficient)
- No foldable sections (drill down instead)
- No permanent help bars (use `?` key for help modal)
- No redundant status panels (use single-line status)

### 3. Information Density

A standard 80x24 terminal must show meaningful content. Target:

- Dashboard menu: ≤ 8 lines (leaves room for context)
- List views: fill available space (20+ items visible)
- Detail views: full terminal width for content
- Status: 1 line at top or bottom

### 4. Predictable Keybindings

- `↑/↓` or `j/k`: Navigate list
- `Enter`: Select / drill down
- `Esc` or `q`: Go back / close
- `?`: Show help modal
- `Ctrl+Q`: Quit

Context-specific actions shown in a single status line.

## Layout Patterns

### Pattern A: Menu Screen

```
→ Workflows
  Agent Loops
  Threads
  Checkpoints
  Settings

↑/↓ navigate  Enter select  ? help  Ctrl+Q quit
```

- Single SelectList filling available space
- Single-line status bar at bottom
- No header, no panels, no cards

### Pattern B: List + Detail (Drill-Down)

List view:
```
→ deploy-api        v2.3  12 nodes  running
  process-queue     v1.1  8 nodes   idle
  notify-slack      v3.0  4 nodes   idle

↑/↓ navigate  Enter detail  N new  D delete  ? help
```

Detail view (on Enter):
```
← Back to Workflows

Name:        deploy-api
Version:     2.3
Status:      running
Nodes:       12
Created:     2024-01-15
Description: Production deployment pipeline

[Enter] view graph  [E] edit  [D] delete  [Esc] back
```

- List shows key fields inline (no side panel needed)
- Detail is a full-screen view, not a side panel
- Navigation: Enter to drill down, Esc to go back

### Pattern C: Monitoring Screen

```
▶ RUNNING  agent-abc123  msgs:42  iter:3

[12:00:01] 🤖 Analyzing your request...
[12:00:02] 🔧 Calling search(query="...")
[12:00:03] ✓ search → 3 results (120ms)
[12:00:04] 🔧 Calling read_file(path="...")
[12:00:05] ✓ read_file → 1.2KB (45ms)
[12:00:06] 🤖 Found the file. Processing...

> Type your message...

↑/↓ scroll history  Esc normal mode  Ctrl+C cancel
```

- Single-line status at top (not a panel)
- Log fills the middle (no foldable sections)
- Input at bottom
- No separate iteration/tool panels — they're inline in the log

## Component Usage Rules

| Component | Usage | Anti-Pattern |
|-----------|-------|--------------|
| `SelectList` | Primary navigation in all list screens | Wrapping in Box |
| `Text` | Status lines, inline info | Using as section header inside a panel |
| `Input` | Chat input, search/filter | Wrapping in Box with padding |
| `Box` | Only for modals/overlays | Wrapping screen sections |
| `Container` | Vertical stacking of flat components | Nesting containers for layout |
| `FoldableSection` | **Do not use** — drill down instead | Hiding content behind fold markers |

## What to Eliminate

1. **Box padding** — Default `Box` constructor should use `paddingX=0, paddingY=0`. Only modals need padding.
2. **FoldableSection** — Replace with drill-down navigation. If content is long, it deserves its own view.
3. **Split layouts** — No left/right panels. Use list → detail drill-down.
4. **Redundant headers** — Panels should not have their own title when the view already has one.
5. **Permanent help bars** — Use `?` key to show help modal.
6. **System metrics on dashboard** — Not relevant to agent CLI workflow.
7. **Recent executions on dashboard** — User came to act, not observe.

## Space Budget (80x24 terminal)

| Screen | Status | Content | Input | Total |
|--------|--------|---------|-------|-------|
| Dashboard | 1 line | 20 lines (menu) | 0 | 21 lines |
| List | 1 line | 21 lines | 0 | 22 lines |
| Detail | 1 line | 21 lines | 0 | 22 lines |
| Agent | 1 line | 19 lines | 2 lines | 22 lines |

Leaves 2 lines for overflow/terminal decoration.
