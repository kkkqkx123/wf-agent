/**
 * Type definitions for keyboard input handling.
 */

// =============================================================================
// Type-Safe Key Identifiers
// =============================================================================

type Letter =
  | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m"
  | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z"
  | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M"
  | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z";

type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";

type SymbolKey =
  | "`" | "-" | "=" | "[" | "]" | "\\" | ";" | "'" | "," | "." | "/"
  | "!" | "@" | "#" | "$" | "%" | "^" | "&" | "*" | "(" | ")" | "_" | "+"
  | "|" | "~" | "{" | "}" | ":" | "<" | ">" | "?";

type SpecialKey =
  | "escape" | "esc" | "enter" | "return" | "tab" | "space" | "backspace"
  | "delete" | "insert" | "clear" | "home" | "end" | "pageUp" | "pageDown"
  | "up" | "down" | "left" | "right"
  | "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12";

type BaseKey = Letter | Digit | SymbolKey | SpecialKey;
type ModifierName = "ctrl" | "shift" | "alt" | "super";

type ModifiedKeyId<Key extends string, RemainingModifiers extends ModifierName = ModifierName> = {
  [M in RemainingModifiers]: `${M}+${Key}` | `${M}+${ModifiedKeyId<Key, Exclude<RemainingModifiers, M>>}`;
}[RemainingModifiers];

/**
 * Union type of all valid key identifiers.
 * Provides autocomplete and catches typos at compile time.
 */
export type KeyId = BaseKey | ModifiedKeyId<BaseKey>;

/**
 * Helper object for creating typed key identifiers with autocomplete.
 */
export const Key = {
  // Special keys
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

  // Symbol keys
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
  exclamation: "!" as const,
  at: "@" as const,
  hash: "#" as const,
  dollar: "$" as const,
  percent: "%" as const,
  caret: "^" as const,
  ampersand: "&" as const,
  asterisk: "*" as const,
  leftparen: "(" as const,
  rightparen: ")" as const,
  underscore: "_" as const,
  plus: "+" as const,
  pipe: "|" as const,
  tilde: "~" as const,
  leftbrace: "{" as const,
  rightbrace: "}" as const,
  colon: ":" as const,
  lessthan: "<" as const,
  greaterthan: ">" as const,
  question: "?" as const,

  // Single modifiers
  ctrl: <K extends BaseKey>(key: K): `ctrl+${K}` => `ctrl+${key}`,
  shift: <K extends BaseKey>(key: K): `shift+${K}` => `shift+${key}`,
  alt: <K extends BaseKey>(key: K): `alt+${K}` => `alt+${key}`,
  super: <K extends BaseKey>(key: K): `super+${K}` => `super+${key}`,

  // Combined modifiers
  ctrlShift: <K extends BaseKey>(key: K): `ctrl+shift+${K}` => `ctrl+shift+${key}`,
  shiftCtrl: <K extends BaseKey>(key: K): `shift+ctrl+${K}` => `shift+ctrl+${key}`,
  ctrlAlt: <K extends BaseKey>(key: K): `ctrl+alt+${K}` => `ctrl+alt+${key}`,
  altCtrl: <K extends BaseKey>(key: K): `alt+ctrl+${K}` => `alt+ctrl+${key}`,
  shiftAlt: <K extends BaseKey>(key: K): `shift+alt+${K}` => `shift+alt+${key}`,
  altShift: <K extends BaseKey>(key: K): `alt+shift+${K}` => `alt+shift+${key}`,
  ctrlSuper: <K extends BaseKey>(key: K): `ctrl+super+${K}` => `ctrl+super+${key}`,
  superCtrl: <K extends BaseKey>(key: K): `super+ctrl+${K}` => `super+ctrl+${key}`,
  shiftSuper: <K extends BaseKey>(key: K): `shift+super+${K}` => `shift+super+${key}`,
  superShift: <K extends BaseKey>(key: K): `super+shift+${K}` => `super+shift+${key}`,
  altSuper: <K extends BaseKey>(key: K): `alt+super+${K}` => `alt+super+${key}`,
  superAlt: <K extends BaseKey>(key: K): `super+alt+${K}` => `super+alt+${key}`,

  // Triple modifiers
  ctrlShiftAlt: <K extends BaseKey>(key: K): `ctrl+shift+alt+${K}` => `ctrl+shift+alt+${key}`,
  ctrlShiftSuper: <K extends BaseKey>(key: K): `ctrl+shift+super+${K}` => `ctrl+shift+super+${key}`,
} as const;

// =============================================================================
// Key Event Types (Kitty Protocol)
// =============================================================================

/**
 * Event types from Kitty keyboard protocol (flag 2)
 * 1 = key press, 2 = key repeat, 3 = key release
 */
export type KeyEventType = "press" | "repeat" | "release";

export interface ParsedKittySequence {
  codepoint: number;
  shiftedKey?: number;
  baseLayoutKey?: number;
  modifier: number;
  eventType: KeyEventType;
}

export interface ParsedModifyOtherKeysSequence {
  codepoint: number;
  modifier: number;
}
