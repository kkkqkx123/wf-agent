import { type KeyId, matchesKey } from "./keys/index.js";

/**
 * Input context for context-aware keybinding routing.
 */
export type InputContext = "global" | "chat" | "selectList" | "modal";

/**
 * Global keybinding registry.
 */
export interface Keybindings {
  // Editor navigation and editing
  "tui.editor.cursorUp": true;
  "tui.editor.cursorDown": true;
  "tui.editor.cursorLeft": true;
  "tui.editor.cursorRight": true;
  "tui.editor.cursorWordLeft": true;
  "tui.editor.cursorWordRight": true;
  "tui.editor.cursorLineStart": true;
  "tui.editor.cursorLineEnd": true;
  "tui.editor.jumpForward": true;
  "tui.editor.jumpBackward": true;
  "tui.editor.pageUp": true;
  "tui.editor.pageDown": true;
  "tui.editor.deleteCharBackward": true;
  "tui.editor.deleteCharForward": true;
  "tui.editor.deleteWordBackward": true;
  "tui.editor.deleteWordForward": true;
  "tui.editor.deleteToLineStart": true;
  "tui.editor.deleteToLineEnd": true;
  "tui.editor.yank": true;
  "tui.editor.yankPop": true;
  "tui.editor.undo": true;
  // Generic input actions
  "tui.input.newLine": true;
  "tui.input.submit": true;
  "tui.input.tab": true;
  "tui.input.copy": true;
  // Generic selection actions
  "tui.select.up": true;
  "tui.select.down": true;
  "tui.select.pageUp": true;
  "tui.select.pageDown": true;
  "tui.select.confirm": true;
  "tui.select.cancel": true;
  // Normal-mode navigation actions
  "tui.navigate.up": true;
  "tui.navigate.down": true;
  "tui.navigate.halfPageUp": true;
  "tui.navigate.halfPageDown": true;
  "tui.navigate.top": true;
  "tui.navigate.bottom": true;
}

export type Keybinding = keyof Keybindings;

export interface KeybindingDefinition {
  defaultKeys: KeyId | KeyId[];
  description?: string;
  /** Context in which this binding is active. Omitted means all contexts. */
  context?: InputContext;
}

export type KeybindingDefinitions = Record<string, KeybindingDefinition>;
export type KeybindingsConfig = Record<string, KeyId | KeyId[] | undefined>;

export const TUI_KEYBINDINGS = {
  "tui.editor.cursorUp": { defaultKeys: "up", description: "Move cursor up", context: "chat" as const },
  "tui.editor.cursorDown": { defaultKeys: "down", description: "Move cursor down", context: "chat" as const },
  "tui.editor.cursorLeft": {
    defaultKeys: ["left", "ctrl+b"],
    description: "Move cursor left",
    context: "chat" as const,
  },
  "tui.editor.cursorRight": {
    defaultKeys: ["right", "ctrl+f"],
    description: "Move cursor right",
    context: "chat" as const,
  },
  "tui.editor.cursorWordLeft": {
    defaultKeys: ["alt+left", "ctrl+left", "alt+b"],
    description: "Move cursor word left",
    context: "chat" as const,
  },
  "tui.editor.cursorWordRight": {
    defaultKeys: ["alt+right", "ctrl+right", "alt+f"],
    description: "Move cursor word right",
    context: "chat" as const,
  },
  "tui.editor.cursorLineStart": {
    defaultKeys: ["home", "ctrl+a"],
    description: "Move to line start",
    context: "chat" as const,
  },
  "tui.editor.cursorLineEnd": {
    defaultKeys: ["end", "ctrl+e"],
    description: "Move to line end",
    context: "chat" as const,
  },
  "tui.editor.jumpForward": {
    defaultKeys: "ctrl+]",
    description: "Jump forward to character",
    context: "chat" as const,
  },
  "tui.editor.jumpBackward": {
    defaultKeys: "ctrl+alt+]",
    description: "Jump backward to character",
    context: "chat" as const,
  },
  "tui.editor.pageUp": { defaultKeys: "pageUp", description: "Page up", context: "chat" as const },
  "tui.editor.pageDown": { defaultKeys: "pageDown", description: "Page down", context: "chat" as const },
  "tui.editor.deleteCharBackward": {
    defaultKeys: "backspace",
    description: "Delete character backward",
    context: "chat" as const,
  },
  "tui.editor.deleteCharForward": {
    defaultKeys: ["delete", "ctrl+d"],
    description: "Delete character forward",
    context: "chat" as const,
  },
  "tui.editor.deleteWordBackward": {
    defaultKeys: ["ctrl+w", "alt+backspace"],
    description: "Delete word backward",
    context: "chat" as const,
  },
  "tui.editor.deleteWordForward": {
    defaultKeys: ["alt+d", "alt+delete"],
    description: "Delete word forward",
    context: "chat" as const,
  },
  "tui.editor.deleteToLineStart": {
    defaultKeys: "ctrl+u",
    description: "Delete to line start",
    context: "chat" as const,
  },
  "tui.editor.deleteToLineEnd": {
    defaultKeys: "ctrl+k",
    description: "Delete to line end",
    context: "chat" as const,
  },
  "tui.editor.yank": { defaultKeys: "ctrl+y", description: "Yank", context: "chat" as const },
  "tui.editor.yankPop": { defaultKeys: "alt+y", description: "Yank pop", context: "chat" as const },
  "tui.editor.undo": { defaultKeys: "ctrl+-", description: "Undo", context: "chat" as const },
  "tui.input.newLine": {
    defaultKeys: "shift+enter",
    description: "Insert newline",
    context: "chat" as const,
  },
  "tui.input.submit": { defaultKeys: "enter", description: "Submit input", context: "chat" as const },
  "tui.input.tab": { defaultKeys: "tab", description: "Tab / autocomplete", context: "chat" as const },
  "tui.input.copy": { defaultKeys: "ctrl+c", description: "Copy selection", context: "global" as const },
  "tui.select.up": { defaultKeys: "up", description: "Move selection up", context: "selectList" as const },
  "tui.select.down": { defaultKeys: "down", description: "Move selection down", context: "selectList" as const },
  "tui.select.pageUp": {
    defaultKeys: "pageUp",
    description: "Selection page up",
    context: "selectList" as const,
  },
  "tui.select.pageDown": {
    defaultKeys: "pageDown",
    description: "Selection page down",
    context: "selectList" as const,
  },
  "tui.select.confirm": {
    defaultKeys: "enter",
    description: "Confirm selection",
    context: "selectList" as const,
  },
  "tui.select.cancel": {
    defaultKeys: ["escape", "ctrl+c"],
    description: "Cancel selection",
    context: "selectList" as const,
  },
  "tui.navigate.up": {
    defaultKeys: ["j", "down"],
    description: "Scroll down (Normal mode)",
    context: "chat" as const,
  },
  "tui.navigate.down": {
    defaultKeys: ["k", "up"],
    description: "Scroll up (Normal mode)",
    context: "chat" as const,
  },
  "tui.navigate.halfPageUp": {
    defaultKeys: "ctrl+u",
    description: "Scroll up half page (Normal mode)",
    context: "chat" as const,
  },
  "tui.navigate.halfPageDown": {
    defaultKeys: "ctrl+d",
    description: "Scroll down half page (Normal mode)",
    context: "chat" as const,
  },
  "tui.navigate.top": {
    defaultKeys: ["g", "ctrl+home"],
    description: "Jump to log top (Normal mode)",
    context: "chat" as const,
  },
  "tui.navigate.bottom": {
    defaultKeys: ["G", "ctrl+end"],
    description: "Jump to latest message (Normal mode)",
    context: "chat" as const,
  },
} as const satisfies KeybindingDefinitions;

export interface KeybindingConflict {
  key: KeyId;
  keybindings: string[];
}

function normalizeKeys(keys: KeyId | KeyId[] | undefined): KeyId[] {
  if (keys === undefined) return [];
  const keyList = Array.isArray(keys) ? keys : [keys];
  const seen = new Set<KeyId>();
  const result: KeyId[] = [];
  for (const key of keyList) {
    if (!seen.has(key)) {
      seen.add(key);
      result.push(key);
    }
  }
  return result;
}

export class KeybindingsManager {
  private definitions: KeybindingDefinitions;
  private userBindings: KeybindingsConfig;
  private keysById = new Map<Keybinding, KeyId[]>();
  private conflicts: KeybindingConflict[] = [];

  constructor(definitions: KeybindingDefinitions, userBindings: KeybindingsConfig = {}) {
    this.definitions = definitions;
    this.userBindings = userBindings;
    this.rebuild();
  }

  private rebuild(): void {
    this.keysById.clear();
    this.conflicts = [];

    const userClaims = new Map<KeyId, Set<Keybinding>>();
    for (const [keybinding, keys] of Object.entries(this.userBindings)) {
      if (!(keybinding in this.definitions)) continue;
      for (const key of normalizeKeys(keys)) {
        const claimants = userClaims.get(key) ?? new Set<Keybinding>();
        claimants.add(keybinding as Keybinding);
        userClaims.set(key, claimants);
      }
    }

    for (const [key, keybindings] of userClaims) {
      if (keybindings.size > 1) {
        this.conflicts.push({ key, keybindings: [...keybindings] });
      }
    }

    for (const [id, definition] of Object.entries(this.definitions)) {
      const userKeys = this.userBindings[id];
      const keys = userKeys === undefined ? normalizeKeys(definition.defaultKeys) : normalizeKeys(userKeys);
      this.keysById.set(id as Keybinding, keys);
    }
  }

  matches(data: string, keybinding: Keybinding, context?: InputContext): boolean {
    // When context is specified, only match bindings active in that context
    if (context !== undefined) {
      const definition = this.definitions[keybinding];
      if (definition?.context !== undefined && definition.context !== context) {
        return false;
      }
    }
    const keys = this.keysById.get(keybinding) ?? [];
    for (const key of keys) {
      if (matchesKey(data, key)) return true;
    }
    return false;
  }

  getKeys(keybinding: Keybinding): KeyId[] {
    return [...(this.keysById.get(keybinding) ?? [])];
  }

  getDefinition(keybinding: Keybinding): KeybindingDefinition {
    return this.definitions[keybinding]!;
  }

  getConflicts(): KeybindingConflict[] {
    return this.conflicts.map((conflict) => ({ ...conflict, keybindings: [...conflict.keybindings] }));
  }

  setUserBindings(userBindings: KeybindingsConfig): void {
    this.userBindings = userBindings;
    this.rebuild();
  }

  getUserBindings(): KeybindingsConfig {
    return { ...this.userBindings };
  }

  getResolvedBindings(): KeybindingsConfig {
    const resolved: KeybindingsConfig = {};
    for (const id of Object.keys(this.definitions)) {
      const keys = this.keysById.get(id as Keybinding) ?? [];
      resolved[id] = keys.length === 1 ? keys[0]! : [...keys];
    }
    return resolved;
  }
}

let globalKeybindings: KeybindingsManager | null = null;

export function setKeybindings(keybindings: KeybindingsManager): void {
  globalKeybindings = keybindings;
}

export function getKeybindings(): KeybindingsManager {
  if (!globalKeybindings) {
    globalKeybindings = new KeybindingsManager(TUI_KEYBINDINGS);
  }
  return globalKeybindings;
}
