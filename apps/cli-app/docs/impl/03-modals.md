# Modals

## Overview

Modals are overlay dialogs that capture user input and are displayed above the main content. They implement both `Component` and `Modal` interfaces.

## Modal Interface

### `Modal` (`src/tui/core/modal.ts:9-16`)

```typescript
interface Modal {
  readonly component: Component;
  handleKey(data: string): ModalAction;
  capturesAllKeys(): boolean;
  onOpen?(): void;
  onClose?(): void;
  closeRequested?(): boolean;
}
```

- `handleKey()` returns `ModalAction.Continue` or `ModalAction.Close`
- `capturesAllKeys()` — if true, intercepts all input (non-capturing modals still get a chance after app-level routing)

### `ModalManager` (`src/tui/modals/modal-manager.ts`)

Manages modal lifecycle:
- `show(modal, options?)` — opens modal
- `close()` — closes top modal
- `closeAll()` — closes all modals
- `activeModal` — top of stack
- `isCapturing` — whether top modal captures all keys
- Auto-cleanup on phase → Idle transition

## Built-in Modals

### `ConfirmModal` (`src/tui/modals/confirm-modal.ts`)

Yes/No confirmation dialog.

- **Static API**: `ConfirmModal.show(tui, title, message): Promise<boolean>`
- **Keys**: Y/Enter = confirm, N/Esc = cancel
- **Layout**: Box-drawing borders, centered, 60-char width
- **Rendering**: Title bar, wrapped message, footer hint

### `PasswordModal` (`src/tui/modals/password-modal.ts`)

Password input dialog with masking.

- **Static API**: `PasswordModal.ask(tui, prompt): Promise<string | null>`
- **Keys**: Enter = submit, Esc/Ctrl+C = cancel
- **Display**: Bullet masking (`•` characters)
- **Layout**: 50-char width, centered

### `ModelPicker` (`src/tui/modals/model-picker.ts`)

Navigable list of LLM models.

- **Static API**: `ModelPicker.pick(tui, models): Promise<ModelItem | null>`
- **Keys**: ↑/↓ navigate, Enter select, Esc cancel
- **Data**: `ModelItem { id, name, provider, description? }`
- **Layout**: 70-char width, 10 items visible, selection arrow (→)

### `SessionPicker` (`src/tui/modals/session-picker.ts`)

Navigable list of sessions.

- **Static API**: `SessionPicker.pick(tui, sessions): Promise<SessionItem | null>`
- **Keys**: ↑/↓ navigate, Enter select, Esc cancel
- **Data**: `SessionItem { id, label, description?, timestamp? }`
- **Layout**: 70-char width, 10 items visible, date display

### `FileViewer` (`src/tui/modals/file-viewer.ts`)

Scrollable file content viewer.

- **Static API**: `FileViewer.view(tui, title, content): Promise<void>`
- **Keys**: j/k scroll, Ctrl+u/d half page, g/G top/bottom, q/Esc close
- **Layout**: 80% width, 25 content lines, line count footer
- **Rendering**: Truncates long lines, pads short lines

### `DiffViewer` (`src/tui/modals/diff-viewer.ts`)

Scrollable diff content viewer with colorization.

- **Static API**: `DiffViewer.view(tui, title, diffLines): Promise<void>`
- **Keys**: j/k scroll, Ctrl+u/d half page, g/G top/bottom, q/Esc close
- **Layout**: 80% width, 25 content lines
- **Colorization**: Green (+), Red (-), Cyan (@@)

## Common Patterns

All modals follow this pattern:

```typescript
static show(tui, ...args): Promise<T> {
  return new Promise<T>((resolve) => {
    const modal = new ModalType(...args, resolve);
    const handle = tui.showModal(modal, { anchor: "center", width: N });
    modal.onClose = () => {
      handle.close();
    };
  });
}
```

Layout uses box-drawing characters:
- `┌─ Title ───┐` top border
- `│ content │` content lines with padding
- `└──────────┘` bottom border
