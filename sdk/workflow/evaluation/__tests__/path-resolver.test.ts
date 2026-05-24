import { describe, it, expect } from "vitest";
import { resolvePath, pathExists, setPath, setArrayItemByKey } from "../path-resolver.js";

describe("resolvePath", () => {
  const obj = {
    user: { name: "Alice", age: 30 },
    items: [
      { id: 1, value: "a" },
      { id: 2, value: "b" },
    ],
    data: { items: [{ name: "first" }] },
  };

  it("should resolve simple path", () => {
    expect(resolvePath("user", obj)).toEqual({ name: "Alice", age: 30 });
  });

  it("should resolve nested path", () => {
    expect(resolvePath("user.name", obj)).toBe("Alice");
  });

  it("should resolve path with array index", () => {
    expect(resolvePath("items[0]", obj)).toEqual({ id: 1, value: "a" });
    expect(resolvePath("items[1].value", obj)).toBe("b");
  });

  it("should resolve deeply nested path with array", () => {
    expect(resolvePath("data.items[0].name", obj)).toBe("first");
  });

  it("should return undefined for non-existent path", () => {
    expect(resolvePath("user.nonexistent", obj)).toBeUndefined();
  });

  it("should return undefined for null or undefined root", () => {
    expect(resolvePath("user", null)).toBeUndefined();
    expect(resolvePath("user", undefined)).toBeUndefined();
  });

  it("should return undefined for empty path", () => {
    expect(resolvePath("", obj)).toBeUndefined();
  });
});

describe("pathExists", () => {
  const obj = { user: { name: "Alice" } };

  it("should return true for existing path", () => {
    expect(pathExists("user.name", obj)).toBe(true);
  });

  it("should return false for non-existing path", () => {
    expect(pathExists("user.age", obj)).toBe(false);
  });

  it("should return false for null root", () => {
    expect(pathExists("user", null)).toBe(false);
  });

  it("should return false for invalid path that throws", () => {
    expect(pathExists("__proto__", obj)).toBe(false);
  });
});

describe("setPath", () => {
  it("should set a simple property", () => {
    const obj: any = { user: {} };
    const result = setPath("user.name", obj, "Alice");
    expect(result).toBe(true);
    expect(obj.user.name).toBe("Alice");
  });

  it("should create intermediate objects", () => {
    const obj: any = {};
    const result = setPath("deeply.nested.path", obj, "value");
    expect(result).toBe(true);
    expect(obj.deeply.nested.path).toBe("value");
  });

  it("should create arrays for indexed paths", () => {
    const obj: any = {};
    const result = setPath("items[0].name", obj, "first");
    expect(result).toBe(true);
    expect(Array.isArray(obj.items)).toBe(true);
    expect(obj.items[0].name).toBe("first");
  });

  it("should extend array if index out of range", () => {
    const obj: any = {};
    const result = setPath("items[2]", obj, "value");
    expect(result).toBe(true);
    expect(Array.isArray(obj.items)).toBe(true);
    expect(obj.items[2]).toBe("value");
  });

  it("should set array items by index at last level", () => {
    const obj: any = { arr: [] };
    setPath("arr[0]", obj, "first");
    expect(obj.arr[0]).toBe("first");
  });

  it("should return false for empty path", () => {
    expect(setPath("", {}, "value")).toBe(false);
  });

  it("should return false for null root", () => {
    expect(setPath("a", null, "value")).toBe(false);
  });

  it("should return false for path with empty parts", () => {
    expect(setPath("valid..invalid", {}, "value")).toBe(false);
  });

  it("should return false if intermediate is not an object", () => {
    const obj: any = { a: "string" };
    expect(setPath("a.b", obj, "value")).toBe(false);
  });
});

describe("setArrayItemByKey", () => {
  it("should update matching item in array", () => {
    const arr = [
      { name: "var1", value: 1 },
      { name: "var2", value: 2 },
    ];
    const result = setArrayItemByKey(arr, "name", "var1", "value", 100);
    expect(result).toBe(true);
    expect(arr[0].value).toBe(100);
    expect(arr[1].value).toBe(2);
  });

  it("should return false if item not found", () => {
    const arr = [{ name: "var1", value: 1 }];
    const result = setArrayItemByKey(arr, "name", "nonexistent", "value", 100);
    expect(result).toBe(false);
  });

  it("should return false for non-array input", () => {
    const result = setArrayItemByKey(null as any, "name", "key", "value", 100);
    expect(result).toBe(false);
  });

  it("should return false for empty key or valueField", () => {
    const arr = [{ name: "var1", value: 1 }];
    expect(setArrayItemByKey(arr, "", "var1", "value", 100)).toBe(false);
    expect(setArrayItemByKey(arr, "name", "var1", "", 100)).toBe(false);
  });
});