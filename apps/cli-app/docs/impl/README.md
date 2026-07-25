# CLI-App Implementation Docs

## Structure

| File | Domain |
|------|--------|
| `01-core-engine.md` | TUI engine, rendering, terminal, phases |
| `02-components.md` | UI components (Box, Text, Input, SelectList, panels) |
| `03-modals.md` | Modal system (Confirm, Password, FileViewer, etc.) |
| `04-screens.md` | Screen implementations (Dashboard, Workflow, Agent) |
| `05-app-bootstrap.md` | Application entry, DI, config, output |
| `06-adapter-command-layer.md` | Adapters, commands, services |
| `07-control-flow-summary.md` | Input routing, render flow, phase transitions |

## Design Philosophy

See `docs/spec/design-philosophy.md`.

Key principles:
- **List-driven navigation** — arrow keys + Enter, no split panels
- **Zero chrome** — no Box padding, no section headers, no permanent help
- **Information density** — fill terminal with data, not decoration
- **Predictable keybindings** — ↑/↓ navigate, Enter select, Esc back, ? help
