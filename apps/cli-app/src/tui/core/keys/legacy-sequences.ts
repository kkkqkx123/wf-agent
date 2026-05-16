/**
 * Legacy terminal sequence handling and xterm modifyOtherKeys support.
 *
 * This module provides compatibility layers for terminals that don't support
 * the modern Kitty keyboard protocol. It handles:
 *
 * 1. Legacy escape sequences (CSI, SS3) from traditional terminals
 * 2. xterm modifyOtherKeys protocol (fallback when Kitty is unavailable)
 * 3. Terminal-specific quirks (e.g., Windows Terminal Backspace behavior)
 *
 * These functions are actively used by matching.ts and parsing.ts to ensure
 * cross-terminal compatibility. Do not remove this module - it's essential
 * for supporting older terminals and SSH sessions.
 */

import {
  LEGACY_KEY_SEQUENCES,
  LEGACY_SHIFT_SEQUENCES,
  LEGACY_CTRL_SEQUENCES,
  MODIFIERS,
} from "./constants.js";
import type { ParsedModifyOtherKeysSequence } from "./types.js";

// =============================================================================
// Legacy Sequence Matching
// =============================================================================

export function matchesLegacySequence(data: string, sequences: readonly string[]): boolean {
  return sequences.includes(data);
}

export function matchesLegacyModifierSequence(
  data: string,
  key: keyof typeof LEGACY_SHIFT_SEQUENCES,
  modifier: number,
): boolean {
  if (modifier === MODIFIERS.shift) {
    return matchesLegacySequence(data, LEGACY_SHIFT_SEQUENCES[key]);
  }
  if (modifier === MODIFIERS.ctrl) {
    return matchesLegacySequence(data, LEGACY_CTRL_SEQUENCES[key]);
  }
  return false;
}

// =============================================================================
// ModifyOtherKeys Parsing (xterm)
// =============================================================================

/**
 * Parse xterm modifyOtherKeys format: CSI 27 ; modifiers ; keycode ~
 * Modifier values are 1-indexed: 2=shift, 3=alt, 5=ctrl, etc.
 */
export function parseModifyOtherKeysSequence(data: string): ParsedModifyOtherKeysSequence | null {
  const match = data.match(/^\x1b\[27;(\d+);(\d+)~$/);
  if (!match) return null;
  const modValue = parseInt(match[1]!, 10);
  const codepoint = parseInt(match[2]!, 10);
  return { codepoint, modifier: modValue - 1 };
}

/**
 * Match xterm modifyOtherKeys format.
 */
export function matchesModifyOtherKeys(data: string, expectedKeycode: number, expectedModifier: number): boolean {
  const parsed = parseModifyOtherKeysSequence(data);
  if (!parsed) return false;
  return parsed.codepoint === expectedKeycode && parsed.modifier === expectedModifier;
}

/**
 * Match printable characters via modifyOtherKeys.
 */
export function matchesPrintableModifyOtherKeys(
  data: string,
  expectedKeycode: number,
  expectedModifier: number,
): boolean {
  if (expectedModifier === 0) return false;
  const parsed = parseModifyOtherKeysSequence(data);
  if (!parsed || parsed.modifier !== expectedModifier) return false;
  
  // Normalize shifted letters
  const normalizeShiftedLetter = (codepoint: number, modifier: number): number => {
    const effectiveModifier = modifier & ~(64 + 128); // LOCK_MASK
    if ((effectiveModifier & MODIFIERS.shift) !== 0 && codepoint >= 65 && codepoint <= 90) {
      return codepoint + 32;
    }
    return codepoint;
  };

  return (
    normalizeShiftedLetter(parsed.codepoint, parsed.modifier) ===
    normalizeShiftedLetter(expectedKeycode, expectedModifier)
  );
}

// =============================================================================
// Windows Terminal Detection
// =============================================================================

export function isWindowsTerminalSession(): boolean {
  return (
    Boolean(process.env['WT_SESSION']) &&
    !process.env['SSH_CONNECTION'] &&
    !process.env['SSH_CLIENT'] &&
    !process.env['SSH_TTY']
  );
}

/**
 * Raw 0x08 (BS) is ambiguous in legacy terminals.
 * - Windows Terminal uses it for Ctrl+Backspace.
 * - Some legacy terminals send it for plain Backspace.
 */
export function matchesRawBackspace(data: string, expectedModifier: number): boolean {
  if (data === "\x7f") return expectedModifier === 0;
  if (data !== "\x08") return false;
  return isWindowsTerminalSession() ? expectedModifier === MODIFIERS.ctrl : expectedModifier === 0;
}
