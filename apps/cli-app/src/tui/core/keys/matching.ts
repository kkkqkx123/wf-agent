/**
 * Core key matching logic.
 * Handles matching of terminal input data against key identifiers.
 */

import { MODIFIERS, CODEPOINTS, ARROW_CODEPOINTS, FUNCTIONAL_CODEPOINTS, SYMBOL_KEYS, LEGACY_KEY_SEQUENCES } from "./constants.js";
import { isKittyProtocolActive } from "./kitty-protocol.js";
import { matchesKittySequence } from "./kitty-protocol.js";
import {
  matchesLegacySequence,
  matchesLegacyModifierSequence,
  matchesModifyOtherKeys,
  matchesPrintableModifyOtherKeys,
  matchesRawBackspace,
} from "./legacy-sequences.js";
import type { KeyId } from "./types.js";

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Get the control character for a key.
 * Uses the universal formula: code & 0x1f (mask to lower 5 bits)
 */
function rawCtrlChar(key: string): string | null {
  const char = key.toLowerCase();
  const code = char.charCodeAt(0);
  if ((code >= 97 && code <= 122) || char === "[" || char === "\\" || char === "]" || char === "_") {
    return String.fromCharCode(code & 0x1f);
  }
  // Handle - as _ (same physical key on US keyboards)
  if (char === "-") {
    return String.fromCharCode(31); // Same as Ctrl+_
  }
  return null;
}

function isDigitKey(key: string): boolean {
  return key >= "0" && key <= "9";
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function formatKeyNameWithModifiers(keyName: string, modifier: number): string | undefined {
  const mods: string[] = [];
  const effectiveMod = modifier & ~(64 + 128); // LOCK_MASK
  const supportedModifierMask = MODIFIERS.shift | MODIFIERS.ctrl | MODIFIERS.alt | MODIFIERS.super;
  if ((effectiveMod & ~supportedModifierMask) !== 0) return undefined;
  if (effectiveMod & MODIFIERS.shift) mods.push("shift");
  if (effectiveMod & MODIFIERS.ctrl) mods.push("ctrl");
  if (effectiveMod & MODIFIERS.alt) mods.push("alt");
  if (effectiveMod & MODIFIERS.super) mods.push("super");
  return mods.length > 0 ? `${mods.join("+")}+${keyName}` : keyName;
}

function parseKeyId(
  keyId: string,
): { key: string; ctrl: boolean; shift: boolean; alt: boolean; super: boolean } | null {
  const parts = keyId.toLowerCase().split("+");
  const key = parts[parts.length - 1];
  if (!key) return null;
  return {
    key,
    ctrl: parts.includes("ctrl"),
    shift: parts.includes("shift"),
    alt: parts.includes("alt"),
    super: parts.includes("super"),
  };
}

// =============================================================================
// Main Matching Function
// =============================================================================

/**
 * Match input data against a key identifier string.
 * 
 * Supported key identifiers:
 * - Single keys: "escape", "tab", "enter", "backspace", "delete", "home", "end", "space"
 * - Arrow keys: "up", "down", "left", "right"
 * - Ctrl combinations: "ctrl+c", "ctrl+z", etc.
 * - Shift combinations: "shift+tab", "shift+enter"
 * - Alt combinations: "alt+enter", "alt+backspace"
 * - Super combinations: "super+k", "super+enter"
 * - Combined modifiers: "shift+ctrl+p", "ctrl+alt+x", "ctrl+super+k"
 */
export function matchesKey(data: string, keyId: KeyId): boolean {
  const parsed = parseKeyId(keyId);
  if (!parsed) return false;

  const { key, ctrl, shift, alt, super: superModifier } = parsed;
  let modifier = 0;
  if (shift) modifier |= MODIFIERS.shift;
  if (alt) modifier |= MODIFIERS.alt;
  if (ctrl) modifier |= MODIFIERS.ctrl;
  if (superModifier) modifier |= MODIFIERS.super;

  switch (key) {
    case "escape":
    case "esc":
      if (modifier !== 0) return false;
      return (
        data === "\x1b" ||
        matchesKittySequence(data, CODEPOINTS.escape, 0) ||
        matchesModifyOtherKeys(data, CODEPOINTS.escape, 0)
      );

    case "space":
      if (!isKittyProtocolActive()) {
        if (modifier === MODIFIERS.ctrl && data === "\x00") {
          return true;
        }
        if (modifier === MODIFIERS.alt && data === "\x1b ") {
          return true;
        }
      }
      if (modifier === 0) {
        return (
          data === " " ||
          matchesKittySequence(data, CODEPOINTS.space, 0) ||
          matchesModifyOtherKeys(data, CODEPOINTS.space, 0)
        );
      }
      return (
        matchesKittySequence(data, CODEPOINTS.space, modifier) ||
        matchesModifyOtherKeys(data, CODEPOINTS.space, modifier)
      );

    case "tab":
      if (modifier === MODIFIERS.shift) {
        return (
          data === "\x1b[Z" ||
          matchesKittySequence(data, CODEPOINTS.tab, MODIFIERS.shift) ||
          matchesModifyOtherKeys(data, CODEPOINTS.tab, MODIFIERS.shift)
        );
      }
      if (modifier === 0) {
        return data === "\t" || matchesKittySequence(data, CODEPOINTS.tab, 0);
      }
      return (
        matchesKittySequence(data, CODEPOINTS.tab, modifier) ||
        matchesModifyOtherKeys(data, CODEPOINTS.tab, modifier)
      );

    case "enter":
    case "return":
      if (modifier === MODIFIERS.shift) {
        // CSI u sequences (standard Kitty protocol)
        if (
          matchesKittySequence(data, CODEPOINTS.enter, MODIFIERS.shift) ||
          matchesKittySequence(data, CODEPOINTS.kpEnter, MODIFIERS.shift)
        ) {
          return true;
        }
        // xterm modifyOtherKeys format (fallback when Kitty protocol not enabled)
        if (matchesModifyOtherKeys(data, CODEPOINTS.enter, MODIFIERS.shift)) {
          return true;
        }
        // When Kitty protocol is active, legacy sequences are custom terminal mappings
        if (isKittyProtocolActive()) {
          return data === "\x1b\r" || data === "\n";
        }
        return false;
      }
      if (modifier === MODIFIERS.alt) {
        // CSI u sequences (standard Kitty protocol)
        if (
          matchesKittySequence(data, CODEPOINTS.enter, MODIFIERS.alt) ||
          matchesKittySequence(data, CODEPOINTS.kpEnter, MODIFIERS.alt)
        ) {
          return true;
        }
        // xterm modifyOtherKeys format (fallback when Kitty protocol not enabled)
        if (matchesModifyOtherKeys(data, CODEPOINTS.enter, MODIFIERS.alt)) {
          return true;
        }
        // \x1b\r is alt+enter only in legacy mode (no Kitty protocol)
        if (!isKittyProtocolActive()) {
          return data === "\x1b\r";
        }
        return false;
      }
      if (modifier === 0) {
        return (
          data === "\r" ||
          (!isKittyProtocolActive() && data === "\n") ||
          data === "\x1bOM" || // SS3 M (numpad enter in some terminals)
          matchesKittySequence(data, CODEPOINTS.enter, 0) ||
          matchesKittySequence(data, CODEPOINTS.kpEnter, 0)
        );
      }
      return (
        matchesKittySequence(data, CODEPOINTS.enter, modifier) ||
        matchesKittySequence(data, CODEPOINTS.kpEnter, modifier) ||
        matchesModifyOtherKeys(data, CODEPOINTS.enter, modifier)
      );

    case "backspace":
      if (modifier === MODIFIERS.alt) {
        if (data === "\x1b\x7f" || data === "\x1b\b") {
          return true;
        }
        return (
          matchesKittySequence(data, CODEPOINTS.backspace, MODIFIERS.alt) ||
          matchesModifyOtherKeys(data, CODEPOINTS.backspace, MODIFIERS.alt)
        );
      }
      if (modifier === MODIFIERS.ctrl) {
        // Legacy raw 0x08 is ambiguous
        if (matchesRawBackspace(data, MODIFIERS.ctrl)) return true;
        return (
          matchesKittySequence(data, CODEPOINTS.backspace, MODIFIERS.ctrl) ||
          matchesModifyOtherKeys(data, CODEPOINTS.backspace, MODIFIERS.ctrl)
        );
      }
      if (modifier === 0) {
        return (
          matchesRawBackspace(data, 0) ||
          matchesKittySequence(data, CODEPOINTS.backspace, 0) ||
          matchesModifyOtherKeys(data, CODEPOINTS.backspace, 0)
        );
      }
      return (
        matchesKittySequence(data, CODEPOINTS.backspace, modifier) ||
        matchesModifyOtherKeys(data, CODEPOINTS.backspace, modifier)
      );

    case "insert":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.insert) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.insert, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "insert", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.insert, modifier);

    case "delete":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.delete) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.delete, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "delete", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.delete, modifier);

    case "clear":
      if (modifier === 0) {
        return matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.clear);
      }
      return matchesLegacyModifierSequence(data, "clear", modifier);

    case "home":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.home) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.home, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "home", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.home, modifier);

    case "end":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.end) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.end, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "end", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.end, modifier);

    case "pageup":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.pageUp) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageUp, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "pageUp", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageUp, modifier);

    case "pagedown":
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.pageDown) ||
          matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageDown, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "pageDown", modifier)) {
        return true;
      }
      return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageDown, modifier);

    case "up":
      if (modifier === MODIFIERS.alt) {
        return data === "\x1bp" || matchesKittySequence(data, ARROW_CODEPOINTS.up, MODIFIERS.alt);
      }
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.up) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.up, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "up", modifier)) {
        return true;
      }
      return matchesKittySequence(data, ARROW_CODEPOINTS.up, modifier);

    case "down":
      if (modifier === MODIFIERS.alt) {
        return data === "\x1bn" || matchesKittySequence(data, ARROW_CODEPOINTS.down, MODIFIERS.alt);
      }
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.down) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.down, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "down", modifier)) {
        return true;
      }
      return matchesKittySequence(data, ARROW_CODEPOINTS.down, modifier);

    case "left":
      if (modifier === MODIFIERS.alt) {
        return (
          data === "\x1b[1;3D" ||
          (!isKittyProtocolActive() && data === "\x1bB") ||
          data === "\x1bb" ||
          matchesKittySequence(data, ARROW_CODEPOINTS.left, MODIFIERS.alt)
        );
      }
      if (modifier === MODIFIERS.ctrl) {
        return (
          data === "\x1b[1;5D" ||
          matchesLegacyModifierSequence(data, "left", MODIFIERS.ctrl) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.left, MODIFIERS.ctrl)
        );
      }
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.left) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.left, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "left", modifier)) {
        return true;
      }
      return matchesKittySequence(data, ARROW_CODEPOINTS.left, modifier);

    case "right":
      if (modifier === MODIFIERS.alt) {
        return (
          data === "\x1b[1;3C" ||
          (!isKittyProtocolActive() && data === "\x1bF") ||
          data === "\x1bf" ||
          matchesKittySequence(data, ARROW_CODEPOINTS.right, MODIFIERS.alt)
        );
      }
      if (modifier === MODIFIERS.ctrl) {
        return (
          data === "\x1b[1;5C" ||
          matchesLegacyModifierSequence(data, "right", MODIFIERS.ctrl) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.right, MODIFIERS.ctrl)
        );
      }
      if (modifier === 0) {
        return (
          matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.right) ||
          matchesKittySequence(data, ARROW_CODEPOINTS.right, 0)
        );
      }
      if (matchesLegacyModifierSequence(data, "right", modifier)) {
        return true;
      }
      return matchesKittySequence(data, ARROW_CODEPOINTS.right, modifier);

    case "f1":
    case "f2":
    case "f3":
    case "f4":
    case "f5":
    case "f6":
    case "f7":
    case "f8":
    case "f9":
    case "f10":
    case "f11":
    case "f12": {
      if (modifier !== 0) {
        return false;
      }
      return matchesLegacySequence(data, LEGACY_KEY_SEQUENCES[key]);
    }
  }

  // Handle single letter/digit keys and symbols
  if (key.length === 1 && ((key >= "a" && key <= "z") || isDigitKey(key) || SYMBOL_KEYS.has(key))) {
    const codepoint = key.charCodeAt(0);
    const rawCtrl = rawCtrlChar(key);
    const isLetter = key >= "a" && key <= "z";
    const isDigit = isDigitKey(key);

    if (modifier === MODIFIERS.ctrl + MODIFIERS.alt && !isKittyProtocolActive() && rawCtrl) {
      // Legacy: ctrl+alt+key is ESC followed by the control character
      if (data === `\x1b${rawCtrl}`) return true;
    }

    if (modifier === MODIFIERS.alt && !isKittyProtocolActive() && (isLetter || isDigit)) {
      // Legacy: alt+letter/digit is ESC followed by the key
      if (data === `\x1b${key}`) return true;
    }

    if (modifier === MODIFIERS.ctrl) {
      // Legacy: ctrl+key sends the control character
      if (rawCtrl && data === rawCtrl) return true;
      return (
        matchesKittySequence(data, codepoint, MODIFIERS.ctrl) ||
        matchesPrintableModifyOtherKeys(data, codepoint, MODIFIERS.ctrl)
      );
    }

    if (modifier === MODIFIERS.shift + MODIFIERS.ctrl) {
      return (
        matchesKittySequence(data, codepoint, MODIFIERS.shift + MODIFIERS.ctrl) ||
        matchesPrintableModifyOtherKeys(data, codepoint, MODIFIERS.shift + MODIFIERS.ctrl)
      );
    }

    if (modifier === MODIFIERS.shift) {
      // Legacy: shift+letter produces uppercase
      if (isLetter && data === key.toUpperCase()) return true;
      return (
        matchesKittySequence(data, codepoint, MODIFIERS.shift) ||
        matchesPrintableModifyOtherKeys(data, codepoint, MODIFIERS.shift)
      );
    }

    if (modifier !== 0) {
      return (
        matchesKittySequence(data, codepoint, modifier) ||
        matchesPrintableModifyOtherKeys(data, codepoint, modifier)
      );
    }

    // Check both raw char and Kitty sequence (needed for release events)
    return data === key || matchesKittySequence(data, codepoint, 0);
  }

  return false;
}
