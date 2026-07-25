# Screens

## Design Philosophy

All screens follow a **flat list navigation** pattern:
- List view: arrow keys navigate, Enter drills down
- Detail view: full-screen, Esc goes back
- Single-line status bar (not panels)
- Zero chrome: no Box padding, no section headers, no permanent help bars

See `docs/spec/design-philosophy.md` for the full design spec.

## Screen Interface

### `Screen` (`src/tui/screens/screen.ts`)

```typescript
interface Screen {
  render(): Component;
  onActivate?(): void;
  onDeactivate?(): void;
  handleInput?(data: string): boolean;
  destroy?(): void;
}
```

## DashboardScreen

### `DashboardScreen` (`src/tui/screens/dashboard-screen.ts`)

Flat menu screen — the simplest possible layout.

#### Layout

```
→ Workflows
  Agent Loops
  Threads
  Checkpoints
  Settings

↑/↓ navigate  Enter select  ? help  Ctrl+Q quit
```

#### Structure

- `SelectList` — single list filling all available space
- `Text` (padding 0,0) — single-line status bar at bottom
- No header, no status panels, no system metrics, no permanent help

#### Implementation Notes

- Menu items are static — no async data loading
- `onActivate()` is a no-op (nothing to refresh)
- Navigation handled entirely by `SelectList.onSelect`

## WorkflowScreen

### `WorkflowScreen` (`src/tui/screens/workflow-screen.ts`)

Two-mode screen: list view and detail view.

#### List View Layout

```
→ deploy-api        v2.3  12 nodes  running
  process-queue     v1.1  8 nodes   idle
  notify-slack      v3.0  4 nodes   idle

↑/↓ navigate  Enter detail  N new  D delete  R refresh  B back
```

#### Detail View Layout

```
← Back to Workflows

Name:        deploy-api
Version:     v2.3
Status:      running
Nodes:       12
Created:     2024-01-15
Description: Production deployment pipeline

[Enter] view graph  [E] edit  [D] delete  [Esc] back
```

#### Data Loading

- `loadWorkflows()` fetches workflows and their node counts in parallel
- Auto-selects first item
- Items display key fields inline: name, version, node count, status

#### Navigation Model

- `showList()` → clears container, adds `SelectList`
- `showDetail(item)` → clears container, builds detail Container
- `Esc` in detail → `showList()`
- `Esc` in list → `onBack()` to dashboard

#### Keybindings

| Key | List View | Detail View |
|-----|-----------|-------------|
| Enter | Drill into detail | — |
| B / Esc | Back to dashboard | Back to list |
| R | Refresh list | — |
| D | — | Delete (with confirm) |
| N | New (placeholder) | — |
| E | — | Edit (placeholder) |

## AgentScreen

### `AgentScreen` (`src/tui/screens/agent-screen.ts`)

Real-time monitoring with flat layout — no foldable sections.

#### Layout

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

#### Structure

- `Text` (padding 0,0) — single-line status at top
- `Container` — log entries (flat list of `Text` components)
- `Input` — message input at bottom

#### Status Line Format

```
{icon} {status}[{phase}]  {agentId}  msgs:{count}
```

Example: `▶ RUNNING [STREAMING]  agent-abc123  msgs:42`

#### Log Entry Format

```
[HH:MM:SS] {icon} {message}
```

Icons: 👤 user, 🤖 assistant, ℹ system, 🔧 tool

#### Phase-Specific Input Routing

**Idle Phase**:
- Input delegated to `messageInput`
- Esc switches to Normal mode

**Streaming Phase**:
- Ctrl+C cancels turn
- All other input consumed (buffered by PhaseManager)

**Approval Phase**:
- Y/Enter approves → Streaming
- N/Esc rejects → Idle
- Ctrl+C cancels

**Normal Mode**:
- j/k scroll log
- Ctrl+u/d half page
- g/G jump top/bottom
- Space or Enter/Esc → back to Idle
- Printable char → Idle + type in input

#### Key Differences from Previous Design

- No `FoldableSection` — content is flat
- No separate iteration/tool panels — all inline in log
- No Box wrappers around sections — zero padding
- Status is 1 line, not a panel
- `rebuildLog()` directly adds `Text` children to Container
