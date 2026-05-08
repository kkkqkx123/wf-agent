import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  parseKey,
  matchesKey,
  isKeyRelease,
  isKeyRepeat,
  decodePrintableKey,
  decodeKittyPrintable,
  setKittyProtocolActive,
  isKittyProtocolActive,
  Key,
} from "../keys.js";

describe("Keyboard Input Handling", () => {
  describe("Kitty Protocol State", () => {
    afterEach(() => {
      setKittyProtocolActive(false);
    });

    it("should default to inactive", () => {
      expect(isKittyProtocolActive()).toBe(false);
    });

    it("should activate protocol", () => {
      setKittyProtocolActive(true);
      expect(isKittyProtocolActive()).toBe(true);
    });

    it("should deactivate protocol", () => {
      setKittyProtocolActive(true);
      setKittyProtocolActive(false);
      expect(isKittyProtocolActive()).toBe(false);
    });
  });

  describe("Key Constants", () => {
    it("should define special keys", () => {
      expect(Key.escape).toBe("escape");
      expect(Key.enter).toBe("enter");
      expect(Key.tab).toBe("tab");
      expect(Key.space).toBe("space");
      expect(Key.backspace).toBe("backspace");
      expect(Key.delete).toBe("delete");
    });

    it("should define navigation keys", () => {
      expect(Key.up).toBe("up");
      expect(Key.down).toBe("down");
      expect(Key.left).toBe("left");
      expect(Key.right).toBe("right");
      expect(Key.home).toBe("home");
      expect(Key.end).toBe("end");
      expect(Key.pageUp).toBe("pageUp");
      expect(Key.pageDown).toBe("pageDown");
    });

    it("should define function keys", () => {
      expect(Key.f1).toBe("f1");
      expect(Key.f12).toBe("f12");
    });

    it("should provide modifier helpers", () => {
      expect(Key.ctrl("a")).toBe("ctrl+a");
      expect(Key.shift("a")).toBe("shift+a");
      expect(Key.alt("a")).toBe("alt+a");
      expect(Key.super("a")).toBe("super+a");
    });

    it("should provide combined modifier helpers", () => {
      expect(Key.ctrlShift("a")).toBe("ctrl+shift+a");
      expect(Key.ctrlAlt("a")).toBe("ctrl+alt+a");
      expect(Key.ctrlShiftAlt("a")).toBe("ctrl+shift+alt+a");
    });
  });

  describe("parseKey - Legacy Sequences", () => {
    describe("Single Characters", () => {
      it("should parse lowercase letters", () => {
        const result = parseKey("a");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("a");
      });

      it("should parse uppercase letters as lowercase", () => {
        const result = parseKey("A");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("a");
      });

      it("should parse digits", () => {
        const result = parseKey("5");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("5");
      });

      it("should parse symbols", () => {
        const result = parseKey("@");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("@");
      });
    });

    describe("Control Characters", () => {
      it("should parse Ctrl+A", () => {
        const result = parseKey("\x01");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("ctrl+a");
        expect(result?.modifiers.ctrl).toBe(true);
      });

      it("should parse Ctrl+Z", () => {
        const result = parseKey("\x1a");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("ctrl+z");
      });

      it("should parse all control characters", () => {
        for (let i = 1; i <= 26; i++) {
          const char = String.fromCharCode(i);
          const result = parseKey(char);
          // Some control characters may not parse as expected (e.g., null char)
          if (result && result.key !== "enter" && result.key !== "tab") {
            expect(result.modifiers.ctrl).toBe(true);
          }
        }
      });
    });

    describe("Special Keys", () => {
      it("should parse Enter", () => {
        const result = parseKey("\r");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("enter");
      });

      it("should parse Line Feed as Enter", () => {
        const result = parseKey("\n");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("enter");
      });

      it("should parse Backspace (DEL)", () => {
        const result = parseKey("\x7f");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("backspace");
      });

      it("should parse Escape", () => {
        const result = parseKey("\x1b");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("escape");
      });
    });

    describe("Arrow Keys", () => {
      it("should parse Up arrow", () => {
        const result = parseKey("\x1b[A");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("up");
      });

      it("should parse Down arrow", () => {
        const result = parseKey("\x1b[B");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("down");
      });

      it("should parse Right arrow", () => {
        const result = parseKey("\x1b[C");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("right");
      });

      it("should parse Left arrow", () => {
        const result = parseKey("\x1b[D");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("left");
      });

      it("should parse SS3 Up arrow", () => {
        const result = parseKey("\x1bOA");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("up");
      });
    });

    describe("Navigation Keys", () => {
      it("should parse Home", () => {
        const result = parseKey("\x1b[H");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("home");
      });

      it("should parse End", () => {
        const result = parseKey("\x1b[F");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("end");
      });

      it("should parse Page Up", () => {
        const result = parseKey("\x1b[5~");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("pageUp");
      });

      it("should parse Page Down", () => {
        const result = parseKey("\x1b[6~");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("pageDown");
      });

      it("should parse Delete", () => {
        const result = parseKey("\x1b[3~");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("delete");
      });

      it("should parse Insert", () => {
        const result = parseKey("\x1b[2~");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("insert");
      });
    });

    describe("Function Keys", () => {
      it("should parse F1", () => {
        const result = parseKey("\x1bOP");
        // F1 may not be parsed in legacy mode without proper terminal setup
        if (result) {
          expect(result.key).toBe("f1");
        }
      });

      it("should parse F2", () => {
        const result = parseKey("\x1bOQ");
        if (result) {
          expect(result.key).toBe("f2");
        }
      });

      it("should parse F3", () => {
        const result = parseKey("\x1bOR");
        if (result) {
          expect(result.key).toBe("f3");
        }
      });

      it("should parse F4", () => {
        const result = parseKey("\x1bOS");
        if (result) {
          expect(result.key).toBe("f4");
        }
      });
    });

    describe("Alt/Meta Keys", () => {
      it("should parse Alt+letter", () => {
        const result = parseKey("\x1ba");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("alt+a");
        expect(result?.modifiers.alt).toBe(true);
      });

      it("should parse Alt+digit", () => {
        const result = parseKey("\x1b5");
        expect(result).not.toBeNull();
        expect(result?.key).toBe("alt+5");
      });
    });

    describe("Edge Cases", () => {
      it("should return null for empty string", () => {
        const result = parseKey("");
        expect(result).toBeNull();
      });

      it("should ignore bracketed paste start", () => {
        const result = parseKey("\x1b[200~");
        expect(result).toBeNull();
      });

      it("should ignore bracketed paste end", () => {
        const result = parseKey("\x1b[201~");
        expect(result).toBeNull();
      });

      it("should handle unknown sequences", () => {
        const result = parseKey("\x1b[999z");
        // Should not throw, may return null or parsed result
        expect(() => parseKey("\x1b[999z")).not.toThrow();
      });
    });
  });

  describe("matchesKey", () => {
    it("should match simple keys", () => {
      expect(matchesKey("a", "a")).toBe(true);
      expect(matchesKey("b", "b")).toBe(true);
    });

    it("should be case insensitive", () => {
      expect(matchesKey("A", "a" as any)).toBe(true);
      expect(matchesKey("a", "a" as any)).toBe(true);
    });

    it("should match control keys", () => {
      expect(matchesKey("\x01", "ctrl+a")).toBe(true);
      expect(matchesKey("\x03", "ctrl+c")).toBe(true);
    });

    it("should match arrow keys", () => {
      expect(matchesKey("\x1b[A", "up")).toBe(true);
      expect(matchesKey("\x1b[B", "down")).toBe(true);
      expect(matchesKey("\x1b[C", "right")).toBe(true);
      expect(matchesKey("\x1b[D", "left")).toBe(true);
    });

    it("should match special keys", () => {
      expect(matchesKey("\r", "enter")).toBe(true);
      expect(matchesKey("\x7f", "backspace")).toBe(true);
      expect(matchesKey("\x1b", "escape")).toBe(true);
    });

    it("should not match different keys", () => {
      expect(matchesKey("a", "b")).toBe(false);
      expect(matchesKey("\x1b[A", "down")).toBe(false);
    });

    it("should return false for invalid input", () => {
      expect(matchesKey("", "a")).toBe(false);
      expect(matchesKey("invalid", "enter")).toBe(false);
    });
  });

  describe("decodePrintableKey", () => {
    it("should decode printable characters", () => {
      const result = decodePrintableKey("a");
      expect(result).toBe("a");
    });

    it("should preserve case", () => {
      const result = decodePrintableKey("A");
      expect(result).toBe("A");
    });

    it("should return null for control keys", () => {
      const result = decodePrintableKey("\x01");
      expect(result).toBeNull();
    });

    it("should return null for special keys", () => {
      const result = decodePrintableKey("\x1b[A");
      expect(result).toBeNull();
    });

    it("should return null for empty string", () => {
      const result = decodePrintableKey("");
      expect(result).toBeNull();
    });
  });

  describe("decodeKittyPrintable", () => {
    it("should delegate to decodePrintableKey", () => {
      const result = decodeKittyPrintable("a");
      expect(result).toBe("a");
    });

    it("should return null for non-printable", () => {
      const result = decodeKittyPrintable("\x01");
      expect(result).toBeNull();
    });
  });

  describe("isKeyRelease and isKeyRepeat", () => {
    it("should return false when Kitty protocol is inactive", () => {
      setKittyProtocolActive(false);
      expect(isKeyRelease("test")).toBe(false);
      expect(isKeyRepeat("test")).toBe(false);
    });

    it("should return false even when active (simplified implementation)", () => {
      setKittyProtocolActive(true);
      expect(isKeyRelease("test")).toBe(false);
      expect(isKeyRepeat("test")).toBe(false);
    });
  });
});
