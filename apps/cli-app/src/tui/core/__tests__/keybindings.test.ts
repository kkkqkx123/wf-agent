import { describe, it, expect, beforeEach } from "vitest";
import {
  KeybindingsManager,
  TUI_KEYBINDINGS,
  type KeybindingDefinitions,
  type KeybindingsConfig,
} from "../keybindings.js";

describe("Keybindings", () => {
  describe("TUI_KEYBINDINGS", () => {
    it("should define editor navigation bindings", () => {
      expect(TUI_KEYBINDINGS["tui.editor.cursorUp"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.editor.cursorDown"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.editor.cursorLeft"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.editor.cursorRight"]).toBeDefined();
    });

    it("should define editor deletion bindings", () => {
      expect(TUI_KEYBINDINGS["tui.editor.deleteCharBackward"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.editor.deleteCharForward"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.editor.deleteWordBackward"]).toBeDefined();
    });

    it("should define input bindings", () => {
      expect(TUI_KEYBINDINGS["tui.input.submit"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.input.newLine"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.input.tab"]).toBeDefined();
    });

    it("should define selection bindings", () => {
      expect(TUI_KEYBINDINGS["tui.select.up"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.select.down"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.select.confirm"]).toBeDefined();
      expect(TUI_KEYBINDINGS["tui.select.cancel"]).toBeDefined();
    });

    it("should have descriptions for all bindings", () => {
      Object.values(TUI_KEYBINDINGS).forEach((binding) => {
        expect(binding.description).toBeDefined();
        expect(binding.description?.length).toBeGreaterThan(0);
      });
    });

    it("should have default keys for all bindings", () => {
      Object.values(TUI_KEYBINDINGS).forEach((binding) => {
        expect(binding.defaultKeys).toBeDefined();
      });
    });
  });

  describe("KeybindingsManager", () => {
    const testDefinitions: KeybindingDefinitions = {
      "test.action1": { defaultKeys: "a", description: "Action 1" },
      "test.action2": { defaultKeys: ["b", "ctrl+b"], description: "Action 2" },
      "test.action3": { defaultKeys: "enter", description: "Action 3" },
    };

    let manager: KeybindingsManager;

    beforeEach(() => {
      manager = new KeybindingsManager(testDefinitions);
    });

    describe("Initialization", () => {
      it("should initialize with definitions", () => {
        expect(manager).toBeDefined();
      });

      it("should accept optional user bindings", () => {
        const userBindings: KeybindingsConfig = {
          "test.action1": "x",
        };
        const customManager = new KeybindingsManager(testDefinitions, userBindings);
        expect(customManager).toBeDefined();
      });
    });

    describe("matches", () => {
      it("should match single key binding", () => {
        expect(manager.matches("a", "test.action1" as any)).toBe(true);
      });

      it("should not match wrong key", () => {
        expect(manager.matches("b", "test.action1" as any)).toBe(false);
      });

      it("should match first of multiple keys", () => {
        expect(manager.matches("b", "test.action2" as any)).toBe(true);
      });

      it("should match second of multiple keys", () => {
        expect(manager.matches("\x02", "test.action2" as any)).toBe(true); // ctrl+b
      });

      it("should return false for undefined binding", () => {
        expect(manager.matches("a", "test.action3" as any)).toBe(false);
      });
    });

    describe("context-aware matches", () => {
      const contextDefinitions: KeybindingDefinitions = {
        "ctx.globalOnly": { defaultKeys: "a", description: "Global only", context: "global" },
        "ctx.chatOnly": { defaultKeys: "b", description: "Chat only", context: "chat" },
        "ctx.selectOnly": { defaultKeys: "c", description: "Select only", context: "selectList" },
        "ctx.unrestricted": { defaultKeys: "d", description: "No context restriction" },
      };

      let contextManager: KeybindingsManager;

      beforeEach(() => {
        contextManager = new KeybindingsManager(contextDefinitions);
      });

      it("should match when context is specified and matches binding context", () => {
        expect(contextManager.matches("a", "ctx.globalOnly" as any, "global")).toBe(true);
        expect(contextManager.matches("b", "ctx.chatOnly" as any, "chat")).toBe(true);
        expect(contextManager.matches("c", "ctx.selectOnly" as any, "selectList")).toBe(true);
      });

      it("should not match when context is specified but differs from binding context", () => {
        expect(contextManager.matches("a", "ctx.globalOnly" as any, "chat")).toBe(false);
        expect(contextManager.matches("b", "ctx.chatOnly" as any, "global")).toBe(false);
        expect(contextManager.matches("c", "ctx.selectOnly" as any, "modal")).toBe(false);
      });

      it("should match unrestricted binding regardless of context", () => {
        expect(contextManager.matches("d", "ctx.unrestricted" as any, "global")).toBe(true);
        expect(contextManager.matches("d", "ctx.unrestricted" as any, "chat")).toBe(true);
        expect(contextManager.matches("d", "ctx.unrestricted" as any, "modal")).toBe(true);
      });

      it("should match when no context is passed (backward-compatible)", () => {
        // When context is omitted, all bindings are matchable regardless of their context field
        expect(contextManager.matches("a", "ctx.globalOnly" as any)).toBe(true);
        expect(contextManager.matches("b", "ctx.chatOnly" as any)).toBe(true);
        expect(contextManager.matches("c", "ctx.selectOnly" as any)).toBe(true);
        expect(contextManager.matches("d", "ctx.unrestricted" as any)).toBe(true);
      });

      it("should not match when wrong key is pressed even if context matches", () => {
        expect(contextManager.matches("x", "ctx.chatOnly" as any, "chat")).toBe(false);
      });
    });

    describe("getKeys", () => {
      it("should return keys for binding", () => {
        const keys = manager.getKeys("test.action1" as any);
        expect(keys).toContain("a");
      });

      it("should return multiple keys", () => {
        const keys = manager.getKeys("test.action2" as any);
        expect(keys.length).toBeGreaterThanOrEqual(2);
      });

      it("should return empty array for invalid binding", () => {
        const keys = manager.getKeys("invalid" as any);
        expect(keys).toEqual([]);
      });

      it("should return copy of keys array", () => {
        const keys1 = manager.getKeys("test.action1" as any);
        const keys2 = manager.getKeys("test.action1" as any);
        expect(keys1).not.toBe(keys2);
      });
    });

    describe("getDefinition", () => {
      it("should return definition for binding", () => {
        const def = manager.getDefinition("test.action1" as any);
        expect(def?.defaultKeys).toBe("a");
        expect(def?.description).toBe("Action 1");
      });

      it("should return undefined for invalid binding", () => {
        const def = manager.getDefinition("invalid" as any);
        expect(def).toBeUndefined();
      });
    });

    describe("getConflicts", () => {
      it("should return empty array when no conflicts", () => {
        const conflicts = manager.getConflicts();
        expect(conflicts).toEqual([]);
      });

      it("should detect conflicts in user bindings", () => {
        const conflictingBindings: KeybindingsConfig = {
          "test.action1": "x",
          "test.action2": "x",
        };
        const conflictManager = new KeybindingsManager(testDefinitions, conflictingBindings);
        
        const conflicts = conflictManager.getConflicts();
        expect(conflicts.length).toBeGreaterThan(0);
        expect(conflicts[0]?.key).toBe("x");
      });

      it("should return copies of conflict arrays", () => {
        const conflictingBindings: KeybindingsConfig = {
          "test.action1": "x",
          "test.action2": "x",
        };
        const conflictManager = new KeybindingsManager(testDefinitions, conflictingBindings);
        
        const conflicts1 = conflictManager.getConflicts();
        const conflicts2 = conflictManager.getConflicts();
        expect(conflicts1[0]?.keybindings).not.toBe(conflicts2[0]?.keybindings);
      });
    });

    describe("setUserBindings", () => {
      it("should update user bindings", () => {
        const newBindings: KeybindingsConfig = {
          "test.action1": "z" as any,
        };
        manager.setUserBindings(newBindings);
        
        expect(manager.matches("z", "test.action1" as any)).toBe(true);
        expect(manager.matches("a", "test.action1" as any)).toBe(false);
      });

      it("should rebuild internal state", () => {
        manager.setUserBindings({ "test.action1": "y" as any });
        const keys = manager.getKeys("test.action1" as any);
        expect(keys).toContain("y");
      });

      it("should clear previous conflicts", () => {
        const conflictingBindings: KeybindingsConfig = {
          "test.action1": "x",
          "test.action2": "x",
        };
        const conflictManager = new KeybindingsManager(testDefinitions, conflictingBindings);
        expect(conflictManager.getConflicts().length).toBeGreaterThan(0);
        
        conflictManager.setUserBindings({});
        expect(conflictManager.getConflicts().length).toBe(0);
      });
    });

    describe("getUserBindings", () => {
      it("should return current user bindings", () => {
        const userBindings: KeybindingsConfig = {
          "test.action1": "c" as any,
        };
        const customManager = new KeybindingsManager(testDefinitions, userBindings);
        
        const retrieved = customManager.getUserBindings();
        expect(retrieved["test.action1"]).toBe("c");
      });

      it("should return copy of bindings", () => {
        const userBindings: KeybindingsConfig = {
          "test.action1": "d" as any,
        };
        const customManager = new KeybindingsManager(testDefinitions, userBindings);
        
        const retrieved1 = customManager.getUserBindings();
        const retrieved2 = customManager.getUserBindings();
        expect(retrieved1).not.toBe(retrieved2);
      });

      it("should return empty object when no user bindings", () => {
        const retrieved = manager.getUserBindings();
        expect(retrieved).toEqual({});
      });
    });

    describe("getResolvedBindings", () => {
      it("should return resolved bindings for all actions", () => {
        const resolved = manager.getResolvedBindings();
        expect(resolved["test.action1"]).toBeDefined();
        expect(resolved["test.action2"]).toBeDefined();
        expect(resolved["test.action3"]).toBeDefined();
      });

      it("should return single key as string", () => {
        const resolved = manager.getResolvedBindings();
        expect(typeof resolved["test.action1"]).toBe("string");
      });

      it("should return multiple keys as array", () => {
        const resolved = manager.getResolvedBindings();
        expect(Array.isArray(resolved["test.action2"])).toBe(true);
      });

      it("should reflect user bindings", () => {
        manager.setUserBindings({ "test.action1": "z" });
        const resolved = manager.getResolvedBindings();
        expect(resolved["test.action1"]).toBe("z");
      });
    });
  });

  describe("Integration", () => {
    it("should work with TUI_KEYBINDINGS", () => {
      const manager = new KeybindingsManager(TUI_KEYBINDINGS);
      
      // Arrow keys are matched by their escape sequences
      expect(manager.matches("\x1b[A", "tui.editor.cursorUp")).toBe(true);
      expect(manager.matches("\x1b[B", "tui.editor.cursorDown")).toBe(true);
      expect(manager.matches("\r", "tui.input.submit")).toBe(true);
    });

    it("should support custom key remapping", () => {
      const customBindings: KeybindingsConfig = {
        "tui.editor.cursorUp": "k",
        "tui.editor.cursorDown": "j",
      };
      
      const manager = new KeybindingsManager(TUI_KEYBINDINGS, customBindings);
      
      expect(manager.matches("k", "tui.editor.cursorUp")).toBe(true);
      expect(manager.matches("j", "tui.editor.cursorDown")).toBe(true);
    });

    it("should handle complex modifier combinations", () => {
      const complexDefinitions: KeybindingDefinitions = {
        "complex.ctrlShift": { defaultKeys: "ctrl+shift+a", description: "Ctrl+Shift+A" },
      };
      
      const manager = new KeybindingsManager(complexDefinitions);
      expect(manager).toBeDefined();
    });
  });
});
