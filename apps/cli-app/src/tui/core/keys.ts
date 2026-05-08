/**
 * Keyboard input handling for terminal applications.
 * Supports both legacy terminal sequences and Kitty keyboard protocol.
 */

// =============================================================================
// Global Kitty Protocol State
// =============================================================================

let _kittyProtocolActive = false;

export function setKittyProtocolActive(active: boolean): void {
  _kittyProtocolActive = active;
}

export function isKittyProtocolActive(): boolean {
  return _kittyProtocolActive;
}

// =============================================================================
// Type-Safe Key Identifiers
// =============================================================================

type Letter = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z";
type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";
type SymbolKey = "`" | "-" | "=" | "[" | "]" | "\\" | ";" | "'" | "," | "." | "/" | "!" | "@" | "#" | "$" | "%" | "^" | "&" | "*" | "(" | ")" | "_" | "+" | "|" | "~" | "{" | "}" | ":" | "<" | ">" | "?";
type SpecialKey =
  | "escape" | "esc" | "enter" | "return" | "tab" | "space" | "backspace" | "delete" | "insert" | "clear"
  | "home" | "end" | "pageUp" | "pageDown" | "up" | "down" | "left" | "right"
  | "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12";

type BaseKey = Letter | Digit | SymbolKey | SpecialKey;
type ModifierName = "ctrl" | "shift" | "alt" | "super";

type ModifiedKeyId<Key extends string, RemainingModifiers extends ModifierName = ModifierName> = {
  [M in RemainingModifiers]: `${M}+${Key}` | `${M}+${ModifiedKeyId<Key, Exclude<RemainingModifiers, M>>}`;
}[RemainingModifiers];

export type KeyId = BaseKey | ModifiedKeyId<BaseKey>;

// Helper object for creating typed key identifiers
export const Key = {
  escape: "escape" as const,
  esc: "esc" as const,
  enter: "enter" as const,
  return: "return" as const,
  tab: "tab" as const,
  space: "space" as const,
  backspace: "backspace" as const,
  delete: "delete" as const,
  insert: "insert" as const,
  clear: "clear" as const,
  home: "home" as const,
  end: "end" as const,
  pageUp: "pageUp" as const,
  pageDown: "pageDown" as const,
  up: "up" as const,
  down: "down" as const,
  left: "left" as const,
  right: "right" as const,
  f1: "f1" as const,
  f2: "f2" as const,
  f3: "f3" as const,
  f4: "f4" as const,
  f5: "f5" as const,
  f6: "f6" as const,
  f7: "f7" as const,
  f8: "f8" as const,
  f9: "f9" as const,
  f10: "f10" as const,
  f11: "f11" as const,
  f12: "f12" as const,
  backtick: "`" as const,
  hyphen: "-" as const,
  equals: "=" as const,
  leftbracket: "[" as const,
  rightbracket: "]" as const,
  backslash: "\\" as const,
  semicolon: ";" as const,
  quote: "'" as const,
  comma: "," as const,
  period: "." as const,
  slash: "/" as const,

  // Modifier helpers
  ctrl: <K extends BaseKey>(key: K): `ctrl+${K}` => `ctrl+${key}`,
  shift: <K extends BaseKey>(key: K): `shift+${K}` => `shift+${key}`,
  alt: <K extends BaseKey>(key: K): `alt+${K}` => `alt+${key}`,
  super: <K extends BaseKey>(key: K): `super+${K}` => `super+${key}`,
  ctrlShift: <K extends BaseKey>(key: K): `ctrl+shift+${K}` => `ctrl+shift+${key}`,
  ctrlAlt: <K extends BaseKey>(key: K): `ctrl+alt+${K}` => `ctrl+alt+${key}`,
  ctrlSuper: <K extends BaseKey>(key: K): `ctrl+super+${K}` => `ctrl+super+${key}`,
  shiftAlt: <K extends BaseKey>(key: K): `shift+alt+${K}` => `shift+alt+${key}`,
  shiftSuper: <K extends BaseKey>(key: K): `shift+super+${K}` => `shift+super+${key}`,
  altSuper: <K extends BaseKey>(key: K): `alt+super+${K}` => `alt+super+${key}`,
  ctrlShiftAlt: <K extends BaseKey>(key: K): `ctrl+shift+alt+${K}` => `ctrl+shift+alt+${key}`,
  ctrlShiftSuper: <K extends BaseKey>(key: K): `ctrl+shift+super+${K}` => `ctrl+shift+super+${key}`,
  ctrlAltSuper: <K extends BaseKey>(key: K): `ctrl+alt+super+${K}` => `ctrl+alt+super+${key}`,
  shiftAltSuper: <K extends BaseKey>(key: K): `shift+alt+super+${K}` => `shift+alt+super+${key}`,
  ctrlShiftAltSuper: <K extends BaseKey>(key: K): `ctrl+shift+alt+super+${K}` => `ctrl+shift+alt+super+${key}`,
};

// =============================================================================
// Key Event Types (Kitty Protocol)
// =============================================================================

export type KeyEventType = "press" | "repeat" | "release";

interface ParsedKey {
  key: KeyId;
  eventType: KeyEventType;
  modifiers: {
    shift: boolean;
    alt: boolean;
    ctrl: boolean;
    super: boolean;
  };
}

// =============================================================================
// Key Parsing
// =============================================================================

/**
 * Parse raw terminal input into a structured key event.
 */
export function parseKey(data: string): ParsedKey | null {
  if (!data || data.length === 0) {
    return null;
  }

  // Handle bracketed paste markers - ignore them
  if (data === "\x1b[200~" || data === "\x1b[201~") {
    return null;
  }

  // Check for Kitty protocol extended sequences
  if (_kittyProtocolActive && data.startsWith("\x1b[")) {
    const kittyMatch = data.match(/^\x1b\[(\d+);(\d+)([~u])$/);
    if (kittyMatch) {
      const [, codeStr, modsStr, terminator] = kittyMatch;
      const code = parseInt(codeStr!, 10);
      const mods = parseInt(modsStr!, 10);
      
      // Parse modifiers (bit flags)
      const shift = (mods & 1) !== 0;
      const alt = (mods & 2) !== 0;
      const ctrl = (mods & 4) !== 0;
      const super_ = (mods & 8) !== 0;

      // Determine event type
      let eventType: KeyEventType = "press";
      if (terminator === "u") {
        // Check if it's a release event (code >= 256 indicates release in some implementations)
        // For now, treat all 'u' terminators as press/repeat
        eventType = "press";
      }

      // Map code to key
      const key = mapKittyCodeToKey(code, shift, alt, ctrl, super_);
      if (key) {
        return {
          key,
          eventType,
          modifiers: { shift, alt, ctrl, super: super_ },
        };
      }
    }
  }

  // Legacy terminal sequences
  return parseLegacyKey(data);
}

/**
 * Map Kitty protocol codes to key identifiers.
 */
function mapKittyCodeToKey(code: number, shift: boolean, alt: boolean, ctrl: boolean, super_: boolean): KeyId | null {
  // Basic keys (codes 1-127 mostly match ASCII)
  if (code >= 32 && code <= 126) {
    let baseKey = String.fromCharCode(code).toLowerCase();
    
    // Apply modifiers
    const modifiers: string[] = [];
    if (shift) modifiers.push("shift");
    if (ctrl) modifiers.push("ctrl");
    if (alt) modifiers.push("alt");
    if (super_) modifiers.push("super");

    if (modifiers.length > 0) {
      return `${modifiers.join("+")}+${baseKey}` as KeyId;
    }
    return baseKey as KeyId;
  }

  // Special keys
  const specialKeys: Record<number, KeyId> = {
    27: "escape",
    13: "enter",
    127: "backspace",
    9: "tab",
    32: "space",
  };

  if (code in specialKeys) {
    return specialKeys[code]!;
  }

  // Function keys and navigation keys use higher codes
  // This is a simplified mapping - full implementation would be more comprehensive
  if (code >= 256) {
    const offset = code - 256;
    if (offset >= 1 && offset <= 12) {
      return `f${offset}` as KeyId;
    }
  }

  return null;
}

/**
 * Parse legacy terminal key sequences.
 */
function parseLegacyKey(data: string): ParsedKey | null {
  // Single printable character
  if (data.length === 1 && data.charCodeAt(0) >= 32) {
    return {
      key: data.toLowerCase() as KeyId,
      eventType: "press",
      modifiers: { shift: false, alt: false, ctrl: false, super: false },
    };
  }

  // Control characters (Ctrl+A through Ctrl+Z)
  if (data.length === 1 && data.charCodeAt(0) < 32 && data.charCodeAt(0) !== 27) {
    const charCode = data.charCodeAt(0);
    if (charCode >= 1 && charCode <= 26) {
      const letter = String.fromCharCode(charCode + 96); // Convert to 'a'-'z'
      return {
        key: `ctrl+${letter}` as KeyId,
        eventType: "press",
        modifiers: { shift: false, alt: false, ctrl: true, super: false },
      };
    }
  }

  // Escape sequences
  if (data.startsWith("\x1b")) {
    // ESC alone
    if (data === "\x1b") {
      return {
        key: "escape",
        eventType: "press",
        modifiers: { shift: false, alt: false, ctrl: false, super: false },
      };
    }

    // Alt+key (ESC followed by character)
    if (data.length === 2 && data.charCodeAt(1) >= 32) {
      return {
        key: `alt+${String.fromCharCode(data.charCodeAt(1)).toLowerCase()}` as KeyId,
        eventType: "press",
        modifiers: { shift: false, alt: true, ctrl: false, super: false },
      };
    }

    // CSI sequences (ESC [ ...)
    if (data.startsWith("\x1b[")) {
      return parseCsiSequence(data);
    }

    // SS3 sequences (ESC O ...)
    if (data.startsWith("\x1bO")) {
      return parseSs3Sequence(data);
    }
  }

  return null;
}

/**
 * Parse CSI (Control Sequence Introducer) sequences.
 */
function parseCsiSequence(data: string): ParsedKey | null {
  // Arrow keys
  if (data === "\x1b[A") return { key: "up", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[B") return { key: "down", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[C") return { key: "right", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[D") return { key: "left", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  // Home/End
  if (data === "\x1b[H") return { key: "home", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[F") return { key: "end", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  // Page Up/Down
  if (data === "\x1b[5~") return { key: "pageUp", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[6~") return { key: "pageDown", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  // Delete/Insert
  if (data === "\x1b[3~") return { key: "delete", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1b[2~") return { key: "insert", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  // F1-F4 (some terminals use different sequences)
  if (data === "\x1bOP") return { key: "f1", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOQ") return { key: "f2", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOR") return { key: "f3", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOS") return { key: "f4", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  return null;
}

/**
 * Parse SS3 (Single Shift Three) sequences.
 */
function parseSs3Sequence(data: string): ParsedKey | null {
  // Additional arrow keys (some terminals)
  if (data === "\x1bOA") return { key: "up", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOB") return { key: "down", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOC") return { key: "right", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };
  if (data === "\x1bOD") return { key: "left", eventType: "press", modifiers: { shift: false, alt: false, ctrl: false, super: false } };

  return null;
}

// =============================================================================
// Key Matching
// =============================================================================

/**
 * Check if input data matches a specific key identifier.
 */
export function matchesKey(data: string, keyId: KeyId): boolean {
  const parsed = parseKey(data);
  if (!parsed) return false;

  // Normalize both to lowercase for comparison
  const normalizedParsed = normalizeKeyId(parsed.key);
  const normalizedTarget = normalizeKeyId(keyId);

  return normalizedParsed === normalizedTarget;
}

/**
 * Normalize a key identifier for comparison.
 */
function normalizeKeyId(keyId: KeyId): string {
  return keyId.toLowerCase().replace(/\s+/g, "");
}

/**
 * Check if the input is a key release event (Kitty protocol).
 */
export function isKeyRelease(data: string): boolean {
  if (!_kittyProtocolActive) return false;
  
  // In Kitty protocol, release events have specific encoding
  // This is a simplified check - full implementation would parse the event type
  return false; // For now, we don't distinguish releases in the simplified version
}

/**
 * Check if the input is a key repeat event (Kitty protocol).
 */
export function isKeyRepeat(data: string): boolean {
  if (!_kittyProtocolActive) return false;
  
  // Simplified - would need full Kitty protocol parsing
  return false;
}

/**
 * Decode printable characters from key data.
 */
export function decodePrintableKey(data: string): string | null {
  const parsed = parseKey(data);
  if (!parsed) return null;

  // Only return printable characters
  if (parsed.key.length === 1 && !parsed.modifiers.ctrl && !parsed.modifiers.alt && !parsed.modifiers.super) {
    return parsed.key;
  }

  return null;
}

/**
 * Decode Kitty printable characters (for non-Latin keyboard layouts).
 */
export function decodeKittyPrintable(data: string): string | null {
  // Simplified version - full implementation would handle base layout keys
  return decodePrintableKey(data);
}
