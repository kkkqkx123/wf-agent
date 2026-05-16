/**
 * Kitty keyboard protocol parsing and matching.
 * See: https://sw.kovidgoyal.net/kitty/keyboard-protocol/
 */

import {
  FUNCTIONAL_CODEPOINTS,
  KITTY_FUNCTIONAL_KEY_EQUIVALENTS,
  MODIFIERS,
  LOCK_MASK,
  SYMBOL_KEYS,
} from "./constants.js";
import type { KeyEventType, ParsedKittySequence } from "./types.js";

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
// Event Type Parsing
// =============================================================================

function parseEventType(eventTypeStr: string | undefined): KeyEventType {
  if (!eventTypeStr) return "press";
  const eventType = parseInt(eventTypeStr, 10);
  if (eventType === 2) return "repeat";
  if (eventType === 3) return "release";
  return "press";
}

// =============================================================================
// Normalization Functions
// =============================================================================

export function normalizeKittyFunctionalCodepoint(codepoint: number): number {
  return KITTY_FUNCTIONAL_KEY_EQUIVALENTS.get(codepoint) ?? codepoint;
}

export function normalizeShiftedLetterIdentityCodepoint(codepoint: number, modifier: number): number {
  const effectiveModifier = modifier & ~LOCK_MASK;
  if ((effectiveModifier & MODIFIERS.shift) !== 0 && codepoint >= 65 && codepoint <= 90) {
    return codepoint + 32;
  }
  return codepoint;
}

// =============================================================================
// Kitty Sequence Parsing
// =============================================================================

/**
 * Parse Kitty protocol sequences.
 * Supports CSI u format with alternate keys (flag 4) and event types (flag 2).
 */
export function parseKittySequence(data: string): ParsedKittySequence | null {
  // CSI u format with alternate keys (flag 4):
  // \x1b[<codepoint>u
  // \x1b[<codepoint>;<mod>u
  // \x1b[<codepoint>;<mod>:<event>u
  // \x1b[<codepoint>:<shifted>;<mod>u
  // \x1b[<codepoint>:<shifted>:<base>;<mod>u
  // \x1b[<codepoint>::<base>;<mod>u (no shifted key, only base)
  const csiUMatch = data.match(/^\x1b\[(\d+)(?::(\d*))?(?::(\d+))?(?:;(\d+))?(?::(\d+))?u$/);
  if (csiUMatch) {
    const codepoint = parseInt(csiUMatch[1]!, 10);
    const shiftedKey = csiUMatch[2] && csiUMatch[2].length > 0 ? parseInt(csiUMatch[2], 10) : undefined;
    const baseLayoutKey = csiUMatch[3] ? parseInt(csiUMatch[3], 10) : undefined;
    const modValue = csiUMatch[4] ? parseInt(csiUMatch[4], 10) : 1;
    const eventType = parseEventType(csiUMatch[5]);
    return { codepoint, shiftedKey, baseLayoutKey, modifier: modValue - 1, eventType };
  }

  // Arrow keys with modifier: \x1b[1;<mod>A/B/C/D or \x1b[1;<mod>:<event>A/B/C/D
  const arrowMatch = data.match(/^\x1b\[1;(\d+)(?::(\d+))?([ABCD])$/);
  if (arrowMatch) {
    const modValue = parseInt(arrowMatch[1]!, 10);
    const eventType = parseEventType(arrowMatch[2]);
    const arrowCodes: Record<string, number> = { A: -1, B: -2, C: -3, D: -4 };
    return { codepoint: arrowCodes[arrowMatch[3]!]!, modifier: modValue - 1, eventType };
  }

  // Functional keys: \x1b[<num>~ or \x1b[<num>;<mod>~ or \x1b[<num>;<mod>:<event>~
  const funcMatch = data.match(/^\x1b\[(\d+)(?:;(\d+))?(?::(\d+))?~$/);
  if (funcMatch) {
    const keyNum = parseInt(funcMatch[1]!, 10);
    const modValue = funcMatch[2] ? parseInt(funcMatch[2], 10) : 1;
    const eventType = parseEventType(funcMatch[3]);
    const funcCodes: Record<number, number> = {
      2: FUNCTIONAL_CODEPOINTS.insert,
      3: FUNCTIONAL_CODEPOINTS.delete,
      5: FUNCTIONAL_CODEPOINTS.pageUp,
      6: FUNCTIONAL_CODEPOINTS.pageDown,
      7: FUNCTIONAL_CODEPOINTS.home,
      8: FUNCTIONAL_CODEPOINTS.end,
    };
    const codepoint = funcCodes[keyNum];
    if (codepoint !== undefined) {
      return { codepoint, modifier: modValue - 1, eventType };
    }
  }

  // Home/End with modifier: \x1b[1;<mod>H/F or \x1b[1;<mod>:<event>H/F
  const homeEndMatch = data.match(/^\x1b\[1;(\d+)(?::(\d+))?([HF])$/);
  if (homeEndMatch) {
    const modValue = parseInt(homeEndMatch[1]!, 10);
    const eventType = parseEventType(homeEndMatch[2]);
    const codepoint = homeEndMatch[3] === "H" ? FUNCTIONAL_CODEPOINTS.home : FUNCTIONAL_CODEPOINTS.end;
    return { codepoint, modifier: modValue - 1, eventType };
  }

  return null;
}

/**
 * Match input against expected Kitty sequence.
 */
export function matchesKittySequence(data: string, expectedCodepoint: number, expectedModifier: number): boolean {
  const parsed = parseKittySequence(data);
  if (!parsed) return false;
  const actualMod = parsed.modifier & ~LOCK_MASK;
  const expectedMod = expectedModifier & ~LOCK_MASK;

  // Check if modifiers match
  if (actualMod !== expectedMod) return false;

  const normalizedCodepoint = normalizeShiftedLetterIdentityCodepoint(
    normalizeKittyFunctionalCodepoint(parsed.codepoint),
    parsed.modifier,
  );
  const normalizedExpectedCodepoint = normalizeShiftedLetterIdentityCodepoint(
    normalizeKittyFunctionalCodepoint(expectedCodepoint),
    expectedModifier,
  );

  // Primary match: codepoint matches directly after normalizing functional keys
  if (normalizedCodepoint === normalizedExpectedCodepoint) return true;

  // Alternate match: use base layout key for non-Latin keyboard layouts.
  // Only fall back to base layout key when the codepoint is NOT already a
  // recognized Latin letter (a-z) or symbol.
  if (parsed.baseLayoutKey !== undefined && parsed.baseLayoutKey === expectedCodepoint) {
    const cp = normalizedCodepoint;
    const isLatinLetter = cp >= 97 && cp <= 122; // a-z
    const isKnownSymbol = SYMBOL_KEYS.has(String.fromCharCode(cp));
    if (!isLatinLetter && !isKnownSymbol) return true;
  }

  return false;
}

// =============================================================================
// Event Type Queries
// =============================================================================

/**
 * Check if the last parsed key event was a key release.
 */
export function isKeyRelease(data: string): boolean {
  // Don't treat bracketed paste content as key release
  if (data.includes("\x1b[200~")) {
    return false;
  }

  // Quick check: release events with flag 2 contain ":3"
  if (
    data.includes(":3u") ||
    data.includes(":3~") ||
    data.includes(":3A") ||
    data.includes(":3B") ||
    data.includes(":3C") ||
    data.includes(":3D") ||
    data.includes(":3H") ||
    data.includes(":3F")
  ) {
    return true;
  }
  return false;
}

/**
 * Check if the last parsed key event was a key repeat.
 */
export function isKeyRepeat(data: string): boolean {
  // Don't treat bracketed paste content as key repeat
  if (data.includes("\x1b[200~")) {
    return false;
  }

  if (
    data.includes(":2u") ||
    data.includes(":2~") ||
    data.includes(":2A") ||
    data.includes(":2B") ||
    data.includes(":2C") ||
    data.includes(":2D") ||
    data.includes(":2H") ||
    data.includes(":2F")
  ) {
    return true;
  }
  return false;
}
