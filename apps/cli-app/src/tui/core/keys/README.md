# Keys Module

Keyboard input handling for terminal applications. Supports both legacy terminal sequences and Kitty keyboard protocol.

## Architecture

This module is split into multiple files for better maintainability:

```
keys/
├── types.ts              # Type definitions (KeyId, Key helper, event types)
├── constants.ts          # Constants and mapping tables
├── kitty-protocol.ts     # Kitty keyboard protocol parsing
├── legacy-sequences.ts   # Legacy terminal sequences & modifyOtherKeys
├── matching.ts           # Core key matching logic
├── parsing.ts            # Key parsing and printable decoding
└── index.ts              # Unified exports
```

## API

### Core Functions

```typescript
import { matchesKey, parseKey, Key } from './keys';

// Check if input matches a key identifier
if (matchesKey(data, Key.ctrl('c'))) {
  // Handle Ctrl+C
}

// Parse input to get key identifier
const keyId = parseKey(data); // Returns "ctrl+c", "enter", etc. or undefined
```

### Kitty Protocol

```typescript
import { setKittyProtocolActive, isKittyProtocolActive, isKeyRelease } from './keys';

// Enable Kitty protocol after detecting terminal support
setKittyProtocolActive(true);

// Check event types (only meaningful with Kitty protocol flag 2)
if (isKeyRelease(data)) {
  // Handle key release
}
```

### Printable Characters

```typescript
import { decodePrintableKey, decodeKittyPrintable } from './keys';

// Decode printable characters (handles non-Latin keyboard layouts)
const char = decodePrintableKey(data);
if (char) {
  // Insert character into editor
}
```

## Supported Protocols

1. **Legacy Terminal Sequences**: Traditional escape sequences (CSI, SS3)
2. **Kitty Keyboard Protocol**: Modern protocol with full modifier support
3. **xterm modifyOtherKeys**: Fallback for terminals without Kitty support

## Key Identifier Format

Key identifiers follow this format:
- Simple keys: `"enter"`, `"escape"`, `"a"`, `"1"`
- With modifiers: `"ctrl+c"`, `"shift+tab"`, `"alt+backspace"`
- Combined modifiers: `"ctrl+shift+p"`, `"ctrl+alt+x"`

Use the `Key` helper for type-safe identifiers:
```typescript
Key.enter           // "enter"
Key.ctrl('c')       // "ctrl+c"
Key.ctrlShift('p')  // "ctrl+shift+p"
Key.alt('backspace') // "alt+backspace"
```

## Terminal Compatibility

The module automatically handles differences between:
- Windows Terminal
- Kitty terminal
- Ghostty
- iTerm2
- GNOME Terminal
- tmux sessions
- SSH connections

## References

- [Kitty Keyboard Protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
- [xterm modifyOtherKeys](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
