/**
 * Key parsing and decoding utilities.
 * Converts terminal input data into key identifiers and printable characters.
 */

import {
  CODEPOINTS,
  ARROW_CODEPOINTS,
  FUNCTIONAL_CODEPOINTS,
  SYMBOL_KEYS,
  MODIFIERS,
  LOCK_MASK,
  LEGACY_SEQUENCE_KEY_IDS,
} from "./constants.js";
import { isKittyProtocolActive } from "./kitty-protocol.js";
import { parseKittySequence, normalizeKittyFunctionalCodepoint, normalizeShiftedLetterIdentityCodepoint } from "./kitty-protocol.js";
import { parseModifyOtherKeysSequence } from "./legacy-sequences.js";
import { isWindowsTerminalSession } from "./legacy-sequences.js";

// =============================================================================
// Helper Functions
// =============================================================================

function formatKeyNameWithModifiers(keyName: string, modifier: number): string | undefined {
  const mods: string[] = [];
  const effectiveMod = modifier & ~LOCK_MASK;
  const supportedModifierMask = MODIFIERS.shift | MODIFIERS.ctrl | MODIFIERS.alt | MODIFIERS.super;
  if ((effectiveMod & ~supportedModifierMask) !== 0) return undefined;
  if (effectiveMod & MODIFIERS.shift) mods.push("shift");
  if (effectiveMod & MODIFIERS.ctrl) mods.push("ctrl");
  if (effectiveMod & MODIFIERS.alt) mods.push("alt");
  if (effectiveMod & MODIFIERS.super) mods.push("super");
  return mods.length > 0 ? `${mods.join("+")}+${keyName}` : keyName;
}

function formatParsedKey(codepoint: number, modifier: number, baseLayoutKey?: number): string | undefined {
  const normalizedCodepoint = normalizeKittyFunctionalCodepoint(codepoint);
  const identityCodepoint = normalizeShiftedLetterIdentityCodepoint(normalizedCodepoint, modifier);

  // Use base layout key only when codepoint is not a recognized Latin letter, digit, or symbol
  const isLatinLetter = identityCodepoint >= 97 && identityCodepoint <= 122; // a-z
  const isDigit = identityCodepoint >= 48 && identityCodepoint <= 57; // 0-9
  const isKnownSymbol = SYMBOL_KEYS.has(String.fromCharCode(identityCodepoint));
  const effectiveCodepoint =
    isLatinLetter || isDigit || isKnownSymbol ? identityCodepoint : (baseLayoutKey ?? identityCodepoint);

  let keyName: string | undefined;
  if (effectiveCodepoint === CODEPOINTS.escape) keyName = "escape";
  else if (effectiveCodepoint === CODEPOINTS.tab) keyName = "tab";
  else if (effectiveCodepoint === CODEPOINTS.enter || effectiveCodepoint === CODEPOINTS.kpEnter) keyName = "enter";
  else if (effectiveCodepoint === CODEPOINTS.space) keyName = "space";
  else if (effectiveCodepoint === CODEPOINTS.backspace) keyName = "backspace";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.delete) keyName = "delete";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.insert) keyName = "insert";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.home) keyName = "home";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.end) keyName = "end";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.pageUp) keyName = "pageUp";
  else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.pageDown) keyName = "pageDown";
  else if (effectiveCodepoint === ARROW_CODEPOINTS.up) keyName = "up";
  else if (effectiveCodepoint === ARROW_CODEPOINTS.down) keyName = "down";
  else if (effectiveCodepoint === ARROW_CODEPOINTS.left) keyName = "left";
  else if (effectiveCodepoint === ARROW_CODEPOINTS.right) keyName = "right";
  else if (effectiveCodepoint >= 48 && effectiveCodepoint <= 57) keyName = String.fromCharCode(effectiveCodepoint);
  else if (effectiveCodepoint >= 97 && effectiveCodepoint <= 122) keyName = String.fromCharCode(effectiveCodepoint);
  else if (SYMBOL_KEYS.has(String.fromCharCode(effectiveCodepoint))) keyName = String.fromCharCode(effectiveCodepoint);

  if (!keyName) return undefined;
  return formatKeyNameWithModifiers(keyName, modifier);
}

// =============================================================================
// Main Parse Function
// =============================================================================

/**
 * Parse input data and return the key identifier if recognized.
 */
export function parseKey(data: string): string | undefined {
  const kitty = parseKittySequence(data);
  if (kitty) {
    return formatParsedKey(kitty.codepoint, kitty.modifier, kitty.baseLayoutKey);
  }

  const modifyOtherKeys = parseModifyOtherKeysSequence(data);
  if (modifyOtherKeys) {
    return formatParsedKey(modifyOtherKeys.codepoint, modifyOtherKeys.modifier);
  }

  // Mode-aware legacy sequences
  // When Kitty protocol is active, ambiguous sequences are interpreted as custom terminal mappings
  if (isKittyProtocolActive()) {
    if (data === "\x1b\r" || data === "\n") return "shift+enter";
  }

  const legacySequenceKeyId = LEGACY_SEQUENCE_KEY_IDS[data];
  if (legacySequenceKeyId) return legacySequenceKeyId;

  // Legacy sequences (used when Kitty protocol is not active, or for unambiguous sequences)
  if (data === "\x1b") return "escape";
  if (data === "\x1c") return "ctrl+\\";
  if (data === "\x1d") return "ctrl+]";
  if (data === "\x1f") return "ctrl+-";
  if (data === "\x1b\x1b") return "ctrl+alt+[";
  if (data === "\x1b\x1c") return "ctrl+alt+\\";
  if (data === "\x1b\x1d") return "ctrl+alt+]";
  if (data === "\x1b\x1f") return "ctrl+alt+-";
  if (data === "\t") return "tab";
  if (data === "\r" || (!isKittyProtocolActive() && data === "\n") || data === "\x1bOM") return "enter";
  if (data === "\x00") return "ctrl+space";
  if (data === " ") return "space";
  if (data === "\x7f") return "backspace";
  if (data === "\x08") return isWindowsTerminalSession() ? "ctrl+backspace" : "backspace";
  if (data === "\x1b[Z") return "shift+tab";
  if (!isKittyProtocolActive() && data === "\x1b\r") return "alt+enter";
  if (!isKittyProtocolActive() && data === "\x1b ") return "alt+space";
  if (data === "\x1b\x7f" || data === "\x1b\b") return "alt+backspace";
  if (!isKittyProtocolActive() && data === "\x1bB") return "alt+left";
  if (!isKittyProtocolActive() && data === "\x1bF") return "alt+right";
  if (!isKittyProtocolActive() && data.length === 2 && data[0] === "\x1b") {
    const code = data.charCodeAt(1);
    if (code >= 1 && code <= 26) {
      return `ctrl+alt+${String.fromCharCode(code + 96)}`;
    }
    // Legacy alt+letter/digit (ESC followed by the key)
    if ((code >= 97 && code <= 122) || (code >= 48 && code <= 57)) {
      return `alt+${String.fromCharCode(code)}`;
    }
  }
  if (data === "\x1b[A") return "up";
  if (data === "\x1b[B") return "down";
  if (data === "\x1b[C") return "right";
  if (data === "\x1b[D") return "left";
  if (data === "\x1b[H" || data === "\x1bOH") return "home";
  if (data === "\x1b[F" || data === "\x1bOF") return "end";
  if (data === "\x1b[3~") return "delete";
  if (data === "\x1b[5~") return "pageUp";
  if (data === "\x1b[6~") return "pageDown";

  // Raw Ctrl+letter
  if (data.length === 1) {
    const code = data.charCodeAt(0);
    if (code >= 1 && code <= 26) {
      return `ctrl+${String.fromCharCode(code + 96)}`;
    }
    if (code >= 32 && code <= 126) {
      return data;
    }
  }

  return undefined;
}

// =============================================================================
// Printable Character Decoding
// =============================================================================

const KITTY_CSI_U_REGEX = /^\x1b\[(\d+)(?::(\d*))?(?::(\d+))?(?:;(\d+))?(?::(\d+))?u$/;
const KITTY_PRINTABLE_ALLOWED_MODIFIERS = MODIFIERS.shift | LOCK_MASK;

/**
 * Decode a Kitty CSI-u sequence into a printable character, if applicable.
 * Only accepts plain or Shift-modified keys. Rejects Ctrl, Alt combinations.
 */
export function decodeKittyPrintable(data: string): string | undefined {
  const match = data.match(KITTY_CSI_U_REGEX);
  if (!match) return undefined;

  const codepoint = Number.parseInt(match[1] ?? "", 10);
  if (!Number.isFinite(codepoint)) return undefined;

  const shiftedKey = match[2] && match[2].length > 0 ? Number.parseInt(match[2], 10) : undefined;
  const modValue = match[4] ? Number.parseInt(match[4], 10) : 1;
  const modifier = Number.isFinite(modValue) ? modValue - 1 : 0;

  // Only accept printable CSI-u input for plain or Shift-modified text keys
  if ((modifier & ~KITTY_PRINTABLE_ALLOWED_MODIFIERS) !== 0) return undefined;
  if (modifier & (MODIFIERS.alt | MODIFIERS.ctrl)) return undefined;

  // Prefer the shifted keycode when Shift is held
  let effectiveCodepoint = codepoint;
  if (modifier & MODIFIERS.shift && typeof shiftedKey === "number") {
    effectiveCodepoint = shiftedKey;
  }
  effectiveCodepoint = normalizeKittyFunctionalCodepoint(effectiveCodepoint);
  
  // Drop control characters or invalid codepoints
  if (!Number.isFinite(effectiveCodepoint) || effectiveCodepoint < 32) return undefined;

  try {
    return String.fromCodePoint(effectiveCodepoint);
  } catch {
    return undefined;
  }
}

function decodeModifyOtherKeysPrintable(data: string): string | undefined {
  const parsed = parseModifyOtherKeysSequence(data);
  if (!parsed) return undefined;
  const modifier = parsed.modifier & ~LOCK_MASK;
  if ((modifier & ~MODIFIERS.shift) !== 0) return undefined;
  if (!Number.isFinite(parsed.codepoint) || parsed.codepoint < 32) return undefined;

  try {
    return String.fromCodePoint(parsed.codepoint);
  } catch {
    return undefined;
  }
}

/**
 * Decode printable characters from key data.
 */
export function decodePrintableKey(data: string): string | undefined {
  return decodeKittyPrintable(data) ?? decodeModifyOtherKeysPrintable(data);
}
