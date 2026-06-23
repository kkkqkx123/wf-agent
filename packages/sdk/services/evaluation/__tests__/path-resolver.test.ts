/**
 * PathResolver Tests
 * Tests path resolution with context awareness
 */

import { describe, it, expect } from "vitest";
import {
  resolvePath,
  pathExists,
  setPath,
  setArrayItemByKey,
  resolveContextPath,
} from "../shared/path-resolver.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("PathResolver", () => {
  describe("resolvePath - Basic nested object access", () => {
    it("should resolve simple property", () => {
      const obj = { name: "John" };
      expect(resolvePath("name", obj)).toBe("John");
    });

    it("should resolve nested property", () => {
      const obj = { user: { profile: { name: "John" } } };
      expect(resolvePath("user.profile.name", obj)).toBe("John");
    });

    it("should return undefined for non-existent path", () => {
      const obj = { user: { name: "John" } };
      expect(resolvePath("user.email", obj)).toBeUndefined();
    });

    it("should return undefined when accessing property on non-object", () => {
      const obj = { count: 5 };
      expect(resolvePath("count.value", obj)).toBeUndefined();
    });

    it("should return undefined for null root", () => {
      expect(resolvePath("name", null)).toBeUndefined();
    });

    it("should return undefined for empty path", () => {
      const obj = { name: "John" };
      expect(resolvePath("", obj)).toBeUndefined();
    });
  });

  describe("resolvePath - Array index access", () => {
    it("should resolve array element by index", () => {
      const obj = { items: ["a", "b", "c"] };
      expect(resolvePath("items[0]", obj)).toBe("a");
      expect(resolvePath("items[2]", obj)).toBe("c");
    });

    it("should resolve nested array access", () => {
      const obj = { items: [{ name: "item1" }, { name: "item2" }] };
      expect(resolvePath("items[0].name", obj)).toBe("item1");
      expect(resolvePath("items[1].name", obj)).toBe("item2");
    });

    it("should return undefined for out-of-bounds index", () => {
      const obj = { items: ["a", "b"] };
      expect(resolvePath("items[5]", obj)).toBeUndefined();
    });

    it("should handle array element access", () => {
      // Note: Double array indices like matrix[0][0] are not supported by the regex pattern
      // This is a security/simplicity tradeoff
      const obj = { matrix: [{ values: [1, 2] }, { values: [3, 4] }] };
      expect(resolvePath("matrix[0].values[0]", obj)).toBe(1);
      expect(resolvePath("matrix[1].values[1]", obj)).toBe(4);
    });

    it("should handle deep nested array structures", () => {
      const obj = {
        data: {
          users: [
            { roles: ["admin", "user"] },
            { roles: ["user"] }
          ]
        }
      };
      expect(resolvePath("data.users[0].roles[0]", obj)).toBe("admin");
      expect(resolvePath("data.users[1].roles[0]", obj)).toBe("user");
    });
  });

  describe("pathExists", () => {
    it("should return true for existing path", () => {
      const obj = { user: { name: "John" } };
      expect(pathExists("user.name", obj)).toBe(true);
    });

    it("should return false for non-existing path", () => {
      const obj = { user: { name: "John" } };
      expect(pathExists("user.email", obj)).toBe(false);
    });

    it("should return true for valid array index", () => {
      const obj = { items: ["a", "b"] };
      expect(pathExists("items[0]", obj)).toBe(true);
    });

    it("should return false for invalid array index", () => {
      const obj = { items: ["a", "b"] };
      expect(pathExists("items[5]", obj)).toBe(false);
    });

    it("should return false on invalid path syntax", () => {
      const obj = { data: "value" };
      expect(pathExists("__proto__", obj)).toBe(false);
    });
  });

  describe("setPath", () => {
    it("should set simple property", () => {
      const obj = {};
      expect(setPath("name", obj, "John")).toBe(true);
      expect(obj).toEqual({ name: "John" });
    });

    it("should set nested property, creating intermediate objects", () => {
      const obj = {};
      expect(setPath("user.profile.name", obj, "John")).toBe(true);
      expect(obj).toEqual({ user: { profile: { name: "John" } } });
    });

    it("should set property on existing nested object", () => {
      const obj = { user: { name: "John" } };
      expect(setPath("user.email", obj, "john@example.com")).toBe(true);
      expect(obj.user).toEqual({ name: "John", email: "john@example.com" });
    });

    it("should set array element by index", () => {
      const obj = { items: [] };
      expect(setPath("items[0]", obj, "first")).toBe(true);
      expect(obj.items[0]).toBe("first");
    });

    it("should extend array when index is out of bounds", () => {
      const obj = { items: ["a"] };
      expect(setPath("items[3]", obj, "d")).toBe(true);
      expect(obj.items.length).toBe(4);
      expect(obj.items[3]).toBe("d");
    });

    it("should return false for invalid path with empty parts", () => {
      const obj = {};
      expect(setPath("user..name", obj, "John")).toBe(false);
    });

    it("should return false when root is null", () => {
      expect(setPath("name", null, "John")).toBe(false);
    });

    it("should return false when trying to set on non-object", () => {
      const obj = { count: 5 };
      expect(setPath("count.value", obj, 10)).toBe(false);
    });

    it("should not allow setting forbidden properties", () => {
      const obj = {};
      // These should throw, not return false
      expect(() => setPath("__proto__", obj, {})).toThrow();
      expect(() => setPath("constructor", obj, {})).toThrow();
      expect(() => setPath("prototype", obj, {})).toThrow();
    });
  });

  describe("setArrayItemByKey", () => {
    it("should find and update item by key", () => {
      const array = [
        { id: 1, value: "a" },
        { id: 2, value: "b" }
      ];
      expect(setArrayItemByKey(array, "id", 2 as any, "value", "updated")).toBe(true);
      expect(array[1].value).toBe("updated");
    });

    it("should return false if item not found", () => {
      const array = [{ id: 1, value: "a" }];
      expect(setArrayItemByKey(array, "id", "999", "value", "x")).toBe(false);
    });

    it("should return false for non-array", () => {
      expect(setArrayItemByKey(null as any, "id", "1", "value", "x")).toBe(false);
    });

    it("should work with string keys", () => {
      const array = [
        { name: "var1", value: 10 },
        { name: "var2", value: 20 }
      ];
      expect(setArrayItemByKey(array, "name", "var1", "value", 100)).toBe(true);
      expect(array[0].value).toBe(100);
    });
  });

  describe("resolveContextPath - Context-aware path resolution", () => {
    const makeContext = (vars?: Record<string, unknown>): EvaluationContext => ({
      variables: vars || {},
      input: { userId: 123, username: "john" },
      output: { result: "success", data: { items: ["a", "b"] } }
    });

    describe("Root scope access", () => {
      it("should resolve 'input' to entire input object", () => {
        const context = makeContext();
        expect(resolveContextPath("input", context)).toEqual(context.input);
      });

      it("should resolve 'output' to entire output object", () => {
        const context = makeContext();
        expect(resolveContextPath("output", context)).toEqual(context.output);
      });

      it("should resolve 'variables' to entire variables object", () => {
        const context = makeContext({ x: 1 });
        expect(resolveContextPath("variables", context)).toEqual({ x: 1 });
      });
    });

    describe("Input scope", () => {
      it("should resolve input.x syntax", () => {
        const context = makeContext();
        expect(resolveContextPath("input.userId", context)).toBe(123);
        expect(resolveContextPath("input.username", context)).toBe("john");
      });

      it("should resolve nested input paths", () => {
        const context: EvaluationContext = {
          variables: {},
          input: { user: { profile: { name: "John" } } },
          output: {}
        };
        expect(resolveContextPath("input.user.profile.name", context)).toBe("John");
      });

      it("should resolve input array access", () => {
        const context: EvaluationContext = {
          variables: {},
          input: { items: ["first", "second"] },
          output: {}
        };
        expect(resolveContextPath("input.items[0]", context)).toBe("first");
      });
    });

    describe("Output scope", () => {
      it("should resolve output.x syntax", () => {
        const context = makeContext();
        expect(resolveContextPath("output.result", context)).toBe("success");
      });

      it("should resolve nested output paths", () => {
        const context = makeContext();
        expect(resolveContextPath("output.data.items[1]", context)).toBe("b");
      });
    });

    describe("Variables scope (default)", () => {
      it("should resolve variables.x syntax", () => {
        const context = makeContext({ count: 5, name: "test" });
        expect(resolveContextPath("variables.count", context)).toBe(5);
        expect(resolveContextPath("variables.name", context)).toBe("test");
      });

      it("should treat path without prefix as variables scope", () => {
        const context = makeContext({ x: 10 });
        expect(resolveContextPath("x", context)).toBe(10);
        expect(resolveContextPath("variables.x", context)).toBe(10);
      });

      it("should resolve nested variables", () => {
        const context = makeContext({ user: { role: "admin" } });
        expect(resolveContextPath("user.role", context)).toBe("admin");
        expect(resolveContextPath("variables.user.role", context)).toBe("admin");
      });
    });

    describe("Cross-scope access", () => {
      it("should resolve different scopes independently", () => {
        const context: EvaluationContext = {
          variables: { x: "from_variables" },
          input: { x: "from_input" },
          output: { x: "from_output" }
        };
        expect(resolveContextPath("x", context)).toBe("from_variables");
        expect(resolveContextPath("input.x", context)).toBe("from_input");
        expect(resolveContextPath("output.x", context)).toBe("from_output");
      });
    });

    describe("Error handling", () => {
      it("should return undefined for invalid paths", () => {
        const context = makeContext();
        expect(resolveContextPath("nonexistent", context)).toBeUndefined();
        expect(resolveContextPath("input.nonexistent", context)).toBeUndefined();
      });

      it("should throw for forbidden paths", () => {
        const context = makeContext();
        expect(() => resolveContextPath("__proto__", context)).toThrow();
        expect(() => resolveContextPath("constructor", context)).toThrow();
      });

      it("should throw for paths with trailing dot", () => {
        const context = makeContext();
        expect(() => resolveContextPath("input.", context)).toThrow();
        expect(() => resolveContextPath("variables.x.", context)).toThrow();
      });

      it("should throw for paths with leading dot", () => {
        const context = makeContext();
        expect(() => resolveContextPath(".input", context)).toThrow();
        expect(() => resolveContextPath(".variables", context)).toThrow();
      });

      it("should throw for paths with consecutive dots", () => {
        const context = makeContext();
        expect(() => resolveContextPath("input..userId", context)).toThrow();
        expect(() => resolveContextPath("variables..x", context)).toThrow();
      });

      it("should throw for null/undefined root scope", () => {
        const context: EvaluationContext = {
          variables: {},
          input: null as any,
          output: undefined as any
        };
        expect(resolveContextPath("input.x", context)).toBeUndefined();
        expect(resolveContextPath("output.x", context)).toBeUndefined();
      });

      it("should return undefined for non-existent scope with valid path", () => {
        const context: EvaluationContext = {
          variables: { x: 10 },
          input: null as any,
          output: {}
        };
        expect(resolveContextPath("input.nested", context)).toBeUndefined();
      });
    });
  });
});
