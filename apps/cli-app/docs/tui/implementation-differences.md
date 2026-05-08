# TUI Implementation Differences Analysis

## Overview

This document analyzes the key differences between our TUI implementation (`apps/cli-app/src/tui`) and the reference implementation (`ref/pi/tui`).

---

## 1. Key Parsing Architecture

### Reference Implementation (ref/pi/tui)

**Return Type**: `parseKey(data: string): string | undefined`
- Returns just the key identifier string (e.g., "right", "ctrl+a")
- Simple, flat structure

**Approach**: Lookup-table driven
```typescript
const LEGACY_SEQUENCE_KEY_IDS: Record<string, KeyId> = {
  "\x1b[C": "right",
  "\x1b[D": "left",
  // ... hundreds of mappings
};

export function parseKey(data: string): string | undefined {
  // 1. Try Kitty protocol parsing
  // 2. Try modifyOtherKeys parsing  
  // 3. Check lookup table for legacy sequences
  // 4. Handle special cases (enter, backspace, etc.)
  // 5. Return raw character if printable
}
```

**Key Features**:
- Extensive sequence mapping table (400+ entries)
- Mode-aware parsing (Kitty protocol active vs inactive)
- Handles Windows Terminal quirks
- Supports shift+enter detection when Kitty is active
- Comprehensive Alt+key handling

### Our Implementation (apps/cli-app/src/tui)

**Return Type**: `parseKey(data: string): ParsedKey | null`
```typescript
interface ParsedKey {
  key: KeyId;
  eventType: KeyEventType;  // "press" | "repeat" | "release"
  modifiers: {
    shift: boolean;
    alt: boolean;
    ctrl: boolean;
    super: boolean;
  };
}
```

**Approach**: Structured parsing with explicit branches
```typescript
export function parseKey(data: string): ParsedKey | null {
  // 1. Parse Kitty CSI-u sequences → structured object
  // 2. Parse legacy sequences → structured object
  // 3. Return null if unrecognized
}
```

**Key Features**:
- Rich structured output with event types and modifier flags
- Simplified sequence handling (~50 sequences vs 400+)
- Missing many edge cases and terminal-specific mappings

---

## 2. decodePrintableKey Behavior

### Critical Difference #1: Case Preservation

**Reference Implementation**:
```typescript
export function decodePrintableKey(data: string): string | undefined {
  return decodeKittyPrintable(data) ?? decodeModifyOtherKeysPrintable(data);
}
```
- Only decodes Kitty/modifyOtherKeys sequences
- For regular characters, returns `undefined`
- Editor falls back to using raw `data` directly

**Our Implementation**:
```typescript
export function decodePrintableKey(data: string): string | null {
  const parsed = parseKey(data);
  if (!parsed) return null;
  
  if (parsed.key.length === 1 && !modifiers) {
    return data;  // ✅ Fixed: now returns original data
  }
  return null;
}
```
- Attempts to parse ALL input through `parseKey`
- **BUG (now fixed)**: Was returning `parsed.key` which was lowercased
- **FIX**: Now returns original `data` to preserve case

### Impact on Tests

This caused test failure: `"should insert at cursor position"`
- Expected: `'Hello WorlXd'` (uppercase X)
- Got: `'Hello Worlxd'` (lowercase x)
- Root cause: `parseKey` lowercases keys for matching, but we need original case for insertion

---

## 3. Special Character Handling

### Backspace (`\x7f`)

**Reference**:
```typescript
if (data === "\x7f") return "backspace";
if (data === "\x08") return isWindowsTerminalSession() ? "ctrl+backspace" : "backspace";
```
- Explicit check early in parseKey
- Handles both DEL (127) and BS (8) codes
- Windows Terminal awareness

**Our Implementation** (Fixed):
```typescript
// Must check BEFORE printable character check
if (data === "\x7f") {
  return { key: "backspace", ... };
}

// Single printable character (excluding DEL)
if (data.length === 1 && data.charCodeAt(0) >= 32 && data.charCodeAt(0) !== 127) {
  return { key: data.toLowerCase(), ... };
}
```
- **BUG (now fixed)**: DEL code 127 was caught by `>= 32` check
- **FIX**: Added explicit backspace check before printable check
- Missing: `\x08` handling and Windows Terminal detection

### Enter/Return

**Reference**:
```typescript
if (data === "\r" || (!_kittyProtocolActive && data === "\n") || data === "\x1bOM") 
  return "enter";
```
- Handles CR, LF (when not in Kitty mode), and SS3 enter
- Mode-aware for shift+enter detection

**Our Implementation** (Fixed):
```typescript
if (data === "\r" || data === "\n") {
  return { key: "enter", ... };
}
```
- **BUG (now fixed)**: `\r` was being treated as Ctrl+M (code 13)
- **FIX**: Added explicit enter check before control character handling
- Missing: `\x1bOM` (SS3 enter) support
- Missing: Mode-aware shift+enter detection

---

## 4. Sequence Coverage Gap

### Reference Has 400+ Mappings, We Have ~50

**Missing in Our Implementation**:

1. **Shift-modified navigation keys**:
   - `\x1b[a` → shift+up
   - `\x1b[b` → shift+down
   - `\x1b[c` → shift+right
   - `\x1b[d` → shift+left

2. **Ctrl-modified navigation keys**:
   - `\x1bOa` → ctrl+up
   - `\x1bOb` → ctrl+down
   - etc.

3. **Multiple F-key sequences**:
   - F1-F4 have 3 different encodings each
   - F5-F12 have multiple variants

4. **Alt+letter/digit combinations**:
   - `\x1bA` through `\x1bZ` → alt+a through alt+z
   - `\x1b0` through `\x1b9` → alt+0 through alt+9

5. **Complex modified keys**:
   - `\x1b[2$` → shift+insert
   - `\x1b[3^` → ctrl+delete
   - `\x1b[5$` → shift+pageUp
   - Many more...

6. **Special sequences**:
   - `\x1b[Z` → shift+tab
   - `\x1b[E` / `\x1bOE` → clear
   - `\x1bOM` → enter (SS3)

---

## 5. Editor handleInput Flow

### Reference Implementation

```typescript
handleInput(data: string): void {
  // 1. Handle jump mode
  // 2. Handle bracketed paste (with state machine)
  // 3. Check keybindings via kb.matches()
  // 4. Try decodePrintableKey() → insertCharacter()
  // 5. Fallback: if charCode >= 32 → insertCharacter(data)
}
```

**Key Point**: Step 5 catches all regular printable characters directly.

### Our Implementation

```typescript
handleInput(data: string): void {
  // 1. Handle bracketed paste (simplified)
  // 2. Check keybindings via kb.matches()
  // 3. Try decodePrintableKey() → insertCharacter()
  // 4. Fallback: if charCode >= 32 → insertCharacter(data)
}
```

**Current State**: Same flow, but our `decodePrintableKey` tries to handle everything instead of just Kitty sequences.

---

## 6. Test Failures Analysis

### Remaining Failures (4 out of 32)

#### Failure 1: "should move cursor right"
```typescript
editor.setText("Hello");
editor.handleInput("\x1b[C"); // Right arrow
expect(editor.getCursor().col).toBe(1); // ❌ Got 5
```

**Issue**: Cursor moving to end instead of one position right.

**Root Cause**: The keybinding system might not be matching correctly, or there's an issue with how the Editor processes the matched action.

#### Failure 2: "should move cursor left"
Similar to above - cursor navigation not working as expected.

#### Failure 3: "should insert text at cursor programmatically"
```typescript
editor.setText("Hello World");
editor.insertTextAtCursor(" Beautiful");
expect(editor.getText()).toBe("Hello Beautiful World");
// ❌ Got: "Hello World Beautiful"
```

**Issue**: `insertTextAtCursor` inserting at end instead of cursor position.

**Root Cause**: Need to check cursor position tracking in `insertTextAtCursor`.

#### Failure 4: "should merge lines on backspace at line start"
```typescript
editor.setText("Line1\nLine2");
editor.handleInput("\x1b[H"); // Home
editor.handleInput("\x7f");   // Backspace
expect(editor.getLines().length).toBe(1); // ❌ Got 2
```

**Issue**: Backspace at line start should merge with previous line.

**Root Cause**: Backspace handler doesn't implement line merging logic.

---

## 7. Recommendations

### Immediate Fixes Needed

1. **Expand sequence coverage**: Add missing key sequences from reference
2. **Fix cursor navigation**: Debug why arrow keys aren't moving cursor correctly
3. **Fix insertTextAtCursor**: Ensure it respects current cursor position
4. **Implement line merging**: Add backspace-at-line-start logic
5. **Add Windows Terminal detection**: For proper backspace handling

### Architectural Improvements

1. **Adopt lookup table approach**: Replace branching logic with comprehensive mapping table
2. **Simplify decodePrintableKey**: Make it only handle Kitty/modifyOtherKeys like reference
3. **Add mode awareness**: Track Kitty protocol state for proper shift+enter detection
4. **Improve test coverage**: Add tests for edge cases and terminal-specific behaviors

### Long-term Considerations

1. **Event type support**: Our structured ParsedKey supports press/release/repeat - leverage this
2. **Modifier tracking**: Our modifier flags are more explicit than reference's bitmask
3. **Extensibility**: Consider making sequence mappings configurable per terminal type

---

## 8. Summary Table

| Feature | Reference | Ours | Status |
|---------|-----------|------|--------|
| Return type | `string \| undefined` | `ParsedKey \| null` | Different design |
| Sequence coverage | 400+ | ~50 | ⚠️ Incomplete |
| Case preservation | ✅ (uses raw data) | ✅ (fixed) | Fixed |
| Backspace handling | ✅ (both \x7f and \x08) | ✅ (\x7f only, fixed) | Partial |
| Enter handling | ✅ (CR, LF, SS3) | ✅ (CR, LF, fixed) | Partial |
| Kitty protocol | ✅ Full support | ✅ Basic support | Functional |
| Mode awareness | ✅ Yes | ❌ No | Missing |
| Windows Terminal | ✅ Detected | ❌ Not detected | Missing |
| Structured events | ❌ No | ✅ Yes | Advantage |

---

## Conclusion

The main issues causing test failures are:
1. **Incomplete sequence mapping** - missing many terminal-specific sequences
2. **Editor logic bugs** - cursor navigation and text insertion not working correctly
3. **Missing features** - line merging, proper backspace behavior

The architectural difference (structured ParsedKey vs simple string) is intentional and provides more information, but requires careful handling to avoid losing original character case.
