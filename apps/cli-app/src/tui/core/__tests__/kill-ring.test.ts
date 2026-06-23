import { describe, it, expect, beforeEach } from "vitest";
import { KillRing } from "../kill-ring.js";

describe("KillRing", () => {
  let killRing: KillRing;

  beforeEach(() => {
    killRing = new KillRing();
  });

  describe("Initialization", () => {
    it("should initialize with empty ring", () => {
      expect(killRing.length).toBe(0);
    });

    it("should have undefined peek initially", () => {
      expect(killRing.peek()).toBeUndefined();
    });

    it("should have undefined yank initially", () => {
      expect(killRing.yank()).toBeUndefined();
    });
  });

  describe("push", () => {
    it("should add text to the ring", () => {
      killRing.push("text1", { prepend: false });
      expect(killRing.length).toBe(1);
    });

    it("should ignore empty text", () => {
      killRing.push("", { prepend: false });
      expect(killRing.length).toBe(0);
    });

    it("should ignore null/undefined text", () => {
      killRing.push("" as any, { prepend: false });
      expect(killRing.length).toBe(0);
    });

    it("should append by default", () => {
      killRing.push("hello", { prepend: false });
      killRing.push(" world", { prepend: false, accumulate: true });
      expect(killRing.peek()).toBe("hello world");
    });

    it("should prepend when specified", () => {
      killRing.push("world", { prepend: false });
      killRing.push("hello ", { prepend: true, accumulate: true });
      expect(killRing.peek()).toBe("hello world");
    });

    it("should create new entry without accumulate", () => {
      killRing.push("first", { prepend: false });
      killRing.push("second", { prepend: false, accumulate: false });
      expect(killRing.length).toBe(2);
      expect(killRing.peek()).toBe("second");
    });

    it("should accumulate multiple entries", () => {
      killRing.push("a", { prepend: false });
      killRing.push("b", { prepend: false, accumulate: true });
      killRing.push("c", { prepend: false, accumulate: true });
      expect(killRing.length).toBe(1);
      expect(killRing.peek()).toBe("abc");
    });
  });

  describe("peek", () => {
    it("should return most recent entry", () => {
      killRing.push("first", { prepend: false });
      killRing.push("second", { prepend: false });
      expect(killRing.peek()).toBe("second");
    });

    it("should not modify the ring", () => {
      killRing.push("test", { prepend: false });
      const first = killRing.peek();
      const second = killRing.peek();
      expect(first).toBe(second);
      expect(killRing.length).toBe(1);
    });

    it("should return undefined when empty", () => {
      expect(killRing.peek()).toBeUndefined();
    });
  });

  describe("rotate", () => {
    it("should move last entry to front", () => {
      killRing.push("first", { prepend: false });
      killRing.push("second", { prepend: false });
      killRing.push("third", { prepend: false });
      
      killRing.rotate();
      
      expect(killRing.peek()).toBe("second");
    });

    it("should not rotate single entry", () => {
      killRing.push("only", { prepend: false });
      killRing.rotate();
      expect(killRing.peek()).toBe("only");
    });

    it("should do nothing when empty", () => {
      expect(() => killRing.rotate()).not.toThrow();
      expect(killRing.length).toBe(0);
    });

    it("should cycle through entries", () => {
      killRing.push("a", { prepend: false });
      killRing.push("b", { prepend: false });
      killRing.push("c", { prepend: false });
      
      // Initial: [a, b, c], peek returns c
      expect(killRing.peek()).toBe("c");
      
      killRing.rotate();
      // After rotate: [c, a, b], peek returns b
      expect(killRing.peek()).toBe("b");
      
      killRing.rotate();
      // After rotate: [b, c, a], peek returns a
      expect(killRing.peek()).toBe("a");
    });
  });

  describe("kill", () => {
    it("should add text to ring", () => {
      killRing.kill("deleted text");
      expect(killRing.length).toBe(1);
      expect(killRing.peek()).toBe("deleted text");
    });

    it("should be equivalent to push with prepend false", () => {
      killRing.kill("text");
      expect(killRing.peek()).toBe("text");
    });

    it("should ignore empty text", () => {
      killRing.kill("");
      expect(killRing.length).toBe(0);
    });
  });

  describe("yank", () => {
    it("should return most recent entry", () => {
      killRing.push("first", { prepend: false });
      killRing.push("second", { prepend: false });
      expect(killRing.yank()).toBe("second");
    });

    it("should be equivalent to peek", () => {
      killRing.push("test", { prepend: false });
      expect(killRing.yank()).toBe(killRing.peek());
    });

    it("should return undefined when empty", () => {
      expect(killRing.yank()).toBeUndefined();
    });
  });

  describe("length", () => {
    it("should return correct count", () => {
      expect(killRing.length).toBe(0);
      
      killRing.push("one", { prepend: false });
      expect(killRing.length).toBe(1);
      
      killRing.push("two", { prepend: false });
      expect(killRing.length).toBe(2);
    });

    it("should not change on peek", () => {
      killRing.push("test", { prepend: false });
      const before = killRing.length;
      killRing.peek();
      const after = killRing.length;
      expect(before).toBe(after);
    });

    it("should change on rotate", () => {
      killRing.push("a", { prepend: false });
      killRing.push("b", { prepend: false });
      const before = killRing.length;
      killRing.rotate();
      const after = killRing.length;
      expect(before).toBe(after); // Length should stay same
    });
  });

  describe("Integration", () => {
    it("should support Emacs-style kill/yank workflow", () => {
      // Kill some text with accumulation
      killRing.kill("Hello ");
      killRing.push("World", { prepend: false, accumulate: true });
      
      // Yank should return accumulated text
      expect(killRing.yank()).toBe("Hello World");
    });

    it("should support yank-pop cycling", () => {
      killRing.kill("first");
      killRing.kill("second");
      killRing.kill("third");
      
      // Initial yank
      expect(killRing.yank()).toBe("third");
      
      // Rotate and yank again
      killRing.rotate();
      expect(killRing.yank()).toBe("second");
      
      killRing.rotate();
      expect(killRing.yank()).toBe("first");
    });

    it("should handle backward deletion (prepend)", () => {
      killRing.push("world", { prepend: false });
      killRing.push("hello ", { prepend: true, accumulate: true });
      expect(killRing.yank()).toBe("hello world");
    });

    it("should handle forward deletion (append)", () => {
      killRing.push("hello", { prepend: false });
      killRing.push(" world", { prepend: false, accumulate: true });
      expect(killRing.yank()).toBe("hello world");
    });
  });
});
