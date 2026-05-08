import { describe, it, expect, beforeEach } from "vitest";
import { UndoStack } from "../undo-stack.js";

describe("UndoStack", () => {
  let stack: UndoStack<string>;

  beforeEach(() => {
    stack = new UndoStack<string>();
  });

  describe("Initialization", () => {
    it("should initialize with empty stack", () => {
      expect(stack.length).toBe(0);
    });

    it("should return undefined on pop when empty", () => {
      expect(stack.pop()).toBeUndefined();
    });

    it("should return undefined on undo when empty", () => {
      expect(stack.undo()).toBeUndefined();
    });
  });

  describe("push", () => {
    it("should add state to stack", () => {
      stack.push("state1");
      expect(stack.length).toBe(1);
    });

    it("should store deep clone of state", () => {
      const obj = { value: 1, nested: { data: "test" } };
      const objStack = new UndoStack<typeof obj>();
      
      objStack.push(obj);
      obj.value = 2;
      obj.nested.data = "modified";
      
      const popped = objStack.pop();
      expect(popped?.value).toBe(1);
      expect(popped?.nested.data).toBe("test");
    });

    it("should handle multiple pushes", () => {
      stack.push("first");
      stack.push("second");
      stack.push("third");
      expect(stack.length).toBe(3);
    });

    it("should handle complex objects", () => {
      interface ComplexState {
        id: number;
        items: string[];
        metadata?: { timestamp: number };
      }
      
      const complexStack = new UndoStack<ComplexState>();
      const state: ComplexState = {
        id: 1,
        items: ["a", "b", "c"],
        metadata: { timestamp: Date.now() },
      };
      
      complexStack.push(state);
      expect(complexStack.length).toBe(1);
    });
  });

  describe("pop", () => {
    it("should remove and return most recent state", () => {
      stack.push("first");
      stack.push("second");
      
      const result = stack.pop();
      expect(result).toBe("second");
      expect(stack.length).toBe(1);
    });

    it("should return states in LIFO order", () => {
      stack.push("first");
      stack.push("second");
      stack.push("third");
      
      expect(stack.pop()).toBe("third");
      expect(stack.pop()).toBe("second");
      expect(stack.pop()).toBe("first");
      expect(stack.length).toBe(0);
    });

    it("should return undefined when empty", () => {
      expect(stack.pop()).toBeUndefined();
    });

    it("should not modify returned state", () => {
      const objStack = new UndoStack<{ value: number }>();
      objStack.push({ value: 42 });
      
      const state = objStack.pop();
      if (state) {
        state.value = 100;
      }
      
      // Stack should be empty, no side effects
      expect(objStack.length).toBe(0);
    });
  });

  describe("undo", () => {
    it("should be equivalent to pop", () => {
      stack.push("test");
      
      const undoResult = stack.undo();
      expect(undoResult).toBe("test");
    });

    it("should return undefined when empty", () => {
      expect(stack.undo()).toBeUndefined();
    });

    it("should support backward compatibility", () => {
      stack.push("state1");
      stack.push("state2");
      
      const undone = stack.undo();
      expect(undone).toBe("state2");
    });
  });

  describe("clear", () => {
    it("should remove all states", () => {
      stack.push("first");
      stack.push("second");
      stack.push("third");
      
      stack.clear();
      expect(stack.length).toBe(0);
    });

    it("should allow reuse after clear", () => {
      stack.push("old");
      stack.clear();
      
      stack.push("new");
      expect(stack.length).toBe(1);
      expect(stack.pop()).toBe("new");
    });

    it("should not throw when clearing empty stack", () => {
      expect(() => stack.clear()).not.toThrow();
    });
  });

  describe("length", () => {
    it("should track number of states", () => {
      expect(stack.length).toBe(0);
      
      stack.push("one");
      expect(stack.length).toBe(1);
      
      stack.push("two");
      expect(stack.length).toBe(2);
      
      stack.pop();
      expect(stack.length).toBe(1);
    });

    it("should update on clear", () => {
      stack.push("test");
      expect(stack.length).toBe(1);
      
      stack.clear();
      expect(stack.length).toBe(0);
    });
  });

  describe("Deep Cloning", () => {
    it("should clone arrays independently", () => {
      const arrStack = new UndoStack<number[]>();
      const arr = [1, 2, 3];
      
      arrStack.push(arr);
      arr.push(4);
      
      const popped = arrStack.pop();
      expect(popped).toEqual([1, 2, 3]);
      expect(popped?.length).toBe(3);
    });

    it("should clone nested objects", () => {
      interface Nested {
        level1: {
          level2: {
            value: string;
          };
        };
      }
      
      const nestedStack = new UndoStack<Nested>();
      const nested: Nested = {
        level1: {
          level2: {
            value: "original",
          },
        },
      };
      
      nestedStack.push(nested);
      nested.level1.level2.value = "modified";
      
      const popped = nestedStack.pop();
      expect(popped?.level1.level2.value).toBe("original");
    });

    it("should clone Date objects", () => {
      const dateStack = new UndoStack<Date>();
      const date = new Date("2024-01-01");
      
      dateStack.push(date);
      
      const popped = dateStack.pop();
      expect(popped?.getTime()).toBe(date.getTime());
    });

    it("should handle null values", () => {
      const nullableStack = new UndoStack<string | null>();
      nullableStack.push(null);
      
      const popped = nullableStack.pop();
      expect(popped).toBeNull();
    });

    it("should handle undefined values", () => {
      const undefinableStack = new UndoStack<string | undefined>();
      undefinableStack.push(undefined);
      
      const popped = undefinableStack.pop();
      expect(popped).toBeUndefined();
    });
  });

  describe("Edge Cases", () => {
    it("should handle large number of states", () => {
      for (let i = 0; i < 1000; i++) {
        stack.push(`state${i}`);
      }
      expect(stack.length).toBe(1000);
    });

    it("should handle repeated same state", () => {
      stack.push("same");
      stack.push("same");
      stack.push("same");
      expect(stack.length).toBe(3);
    });

    it("should handle empty string state", () => {
      stack.push("");
      expect(stack.length).toBe(1);
      expect(stack.pop()).toBe("");
    });

    it("should handle zero as state", () => {
      const numStack = new UndoStack<number>();
      numStack.push(0);
      expect(numStack.length).toBe(1);
      expect(numStack.pop()).toBe(0);
    });

    it("should handle false as state", () => {
      const boolStack = new UndoStack<boolean>();
      boolStack.push(false);
      expect(boolStack.length).toBe(1);
      expect(boolStack.pop()).toBe(false);
    });
  });

  describe("Integration", () => {
    it("should support typical undo workflow", () => {
      interface EditorState {
        text: string;
        cursor: number;
      }
      
      const editorStack = new UndoStack<EditorState>();
      
      // Simulate editing
      editorStack.push({ text: "Hello", cursor: 5 });
      editorStack.push({ text: "Hello World", cursor: 11 });
      editorStack.push({ text: "Hello World!", cursor: 12 });
      
      // Undo last change - should return the previous state
      const undone = editorStack.undo();
      expect(undone?.text).toBe("Hello World!");
      expect(editorStack.length).toBe(2);
      
      // Undo again to get to "Hello World"
      const undone2 = editorStack.undo();
      expect(undone2?.text).toBe("Hello World");
    });

    it("should maintain state independence", () => {
      interface State {
        counter: number;
      }
      
      const stateStack = new UndoStack<State>();
      
      stateStack.push({ counter: 1 });
      stateStack.push({ counter: 2 });
      
      const state2 = stateStack.pop();
      if (state2) {
        state2.counter = 999;
      }
      
      const state1 = stateStack.pop();
      expect(state1?.counter).toBe(1);
    });
  });
});
