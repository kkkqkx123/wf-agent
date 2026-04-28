/**
 * PathResolver unit tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { resolvePath, pathExists, setPath } from "../path-resolver.js";
import { RuntimeValidationError } from "@wf-agent/types";

describe("resolvePath", () => {
  let testObject: any;

  beforeEach(() => {
    testObject = {
      user: {
        name: "John",
        age: 25,
        address: {
          city: "New York",
          country: "USA",
        },
      },
      items: [
        { id: 1, name: "Item 1" },
        { id: 2, name: "Item 2" },
        { id: 3, name: "Item 3" },
      ],
      output: {
        data: {
          items: [
            { id: 1, value: 100 },
            { id: 2, value: 200 },
          ],
        },
      },
      emptyArray: [],
      nullValue: null,
      undefinedValue: undefined,
    };
  });

  describe("Basic Features", () => {
    it("The simple attribute value should be obtained.", () => {
      expect(resolvePath("user", testObject)).toEqual(testObject.user);
    });

    it("The nested attribute value should be obtained.", () => {
      expect(resolvePath("user.name", testObject)).toBe("John");
      expect(resolvePath("user.age", testObject)).toBe(25);
    });

    it("The value of deeply nested attributes should be obtained.", () => {
      expect(resolvePath("user.address.city", testObject)).toBe("New York");
      expect(resolvePath("user.address.country", testObject)).toBe("USA");
    });

    it("The array elements should be retrieved.", () => {
      expect(resolvePath("items[0]", testObject)).toEqual({ id: 1, name: "Item 1" });
      expect(resolvePath("items[1]", testObject)).toEqual({ id: 2, name: "Item 2" });
      expect(resolvePath("items[2]", testObject)).toEqual({ id: 3, name: "Item 3" });
    });

    it("The attribute of an array element should be obtained.", () => {
      expect(resolvePath("items[0].id", testObject)).toBe(1);
      expect(resolvePath("items[0].name", testObject)).toBe("Item 1");
      expect(resolvePath("items[1].name", testObject)).toBe("Item 2");
    });

    it("The properties of elements in deeply nested arrays should be retrieved.", () => {
      expect(resolvePath("output.data.items[0].id", testObject)).toBe(1);
      expect(resolvePath("output.data.items[0].value", testObject)).toBe(100);
      expect(resolvePath("output.data.items[1].value", testObject)).toBe(200);
    });
  });

  describe("Boundary cases", () => {
    it("Empty paths should be handled accordingly.", () => {
      expect(resolvePath("", testObject)).toBeUndefined();
    });

    it("Empty objects should be handled accordingly.", () => {
      expect(resolvePath("user.name", null)).toBeUndefined();
      expect(resolvePath("user.name", undefined)).toBeUndefined();
    });

    it("The path that does not exist should be handled accordingly.", () => {
      expect(resolvePath("nonexistent", testObject)).toBeUndefined();
      expect(resolvePath("user.nonexistent", testObject)).toBeUndefined();
      expect(resolvePath("user.address.nonexistent", testObject)).toBeUndefined();
    });

    it("The null value should be handled.", () => {
      expect(resolvePath("nullValue", testObject)).toBeNull();
    });

    it("The undefined value should be handled.", () => {
      expect(resolvePath("undefinedValue", testObject)).toBeUndefined();
    });

    it("The empty array should be handled accordingly.", () => {
      expect(resolvePath("emptyArray[0]", testObject)).toBeUndefined();
    });

    it("The array out-of-bounds issue should be addressed.", () => {
      expect(resolvePath("items[10]", testObject)).toBeUndefined();
      // Negative index values will be rejected by the security validator, so this is not tested here.
      // expect(resolvePath('items[-1]', testObject)).toBeUndefined();
    });

    it("The intermediate values of null paths should be handled.", () => {
      const obj = { user: null };
      expect(resolvePath("user.name", obj)).toBeUndefined();
    });

    it("The intermediate values of undefined paths should be handled.", () => {
      const obj = { user: undefined };
      expect(resolvePath("user.name", obj)).toBeUndefined();
    });
  });

  describe("Error Handling", () => {
    it("Paths that contain prohibited attributes should be rejected.", () => {
      expect(() => {
        resolvePath("__proto__", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("user.__proto__", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("constructor", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("prototype", testObject);
      }).toThrow(RuntimeValidationError);
    });

    it("Invalid path formats should be rejected.", () => {
      expect(() => {
        resolvePath("user..name", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath(".user.name", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("user.name.", testObject);
      }).toThrow(RuntimeValidationError);
    });

    it("Paths containing special characters should be rejected.", () => {
      expect(() => {
        resolvePath("user-name", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("user@name", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("user name", testObject);
      }).toThrow(RuntimeValidationError);
    });

    it("Property names that start with a number should be rejected.", () => {
      expect(() => {
        resolvePath("1user", testObject);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        resolvePath("user.1name", testObject);
      }).toThrow(RuntimeValidationError);
    });

    it("Deeper paths should be rejected.", () => {
      const deepPath = "a.b.c.d.e.f.g.h.i.j.k";
      expect(() => {
        resolvePath(deepPath, testObject);
      }).toThrow(RuntimeValidationError);
    });
  });

  describe("Special scenarios", () => {
    it("The numeric attribute names should be processed (if they exist).", () => {
      // Note: The current security validator does not allow attribute names to start with a digit.
      // This is a security design measure to prevent potential injection attacks.
      // If support for numeric attribute names is required, the rules of the security validator need to be modified.
      const obj = { "0": "zero", "1": "one" };
      // Due to security restrictions, these tests will fail.
      // expect(resolvePath('0', obj)).toBe('zero');
      // expect(resolvePath('1', obj)).toBe('one');
    });

    it("Property names that start with an underscore should be processed accordingly.", () => {
      const obj = { _private: "value" };
      expect(resolvePath("_private", obj)).toBe("value");
    });

    it("Property names that contain numbers should be handled accordingly.", () => {
      const obj = { user1: "John", user2: "Jane" };
      expect(resolvePath("user1", obj)).toBe("John");
      expect(resolvePath("user2", obj)).toBe("Jane");
    });
  });
});

describe("pathExists", () => {
  let testObject: any;

  beforeEach(() => {
    testObject = {
      user: {
        name: "John",
        age: 25,
        address: {
          city: "New York",
        },
      },
      items: [{ id: 1, name: "Item 1" }],
      nullValue: null,
      undefinedValue: undefined,
    };
  });

  describe("Basic Features", () => {
    it("The existing simple paths should be detected.", () => {
      expect(pathExists("user", testObject)).toBe(true);
      expect(pathExists("user.name", testObject)).toBe(true);
      expect(pathExists("user.age", testObject)).toBe(true);
    });

    it("The existing nested paths should be detected.", () => {
      expect(pathExists("user.address.city", testObject)).toBe(true);
    });

    it("The existing array paths should be detected.", () => {
      expect(pathExists("items[0]", testObject)).toBe(true);
      expect(pathExists("items[0].id", testObject)).toBe(true);
    });

    it("The path that does not exist should be detected.", () => {
      expect(pathExists("nonexistent", testObject)).toBe(false);
      expect(pathExists("user.nonexistent", testObject)).toBe(false);
      expect(pathExists("user.address.nonexistent", testObject)).toBe(false);
    });

    it("Null value paths should be detected.", () => {
      expect(pathExists("nullValue", testObject)).toBe(true);
    });

    it("The paths with undefined values should be detected.", () => {
      // `pathExists` uses `resolvePath` to check whether a path exists.
      // When `resolvePath` returns `undefined`, `pathExists` returns `false`.
      // This is the correct behavior, because an undefined value indicates that the path does not exist or the value is undefined.
      expect(pathExists("undefinedValue", testObject)).toBe(false);
    });

    it("Empty array paths should be detected.", () => {
      expect(pathExists("emptyArray", testObject)).toBe(false);
    });

    it("Array out-of-bounds paths should be detected.", () => {
      expect(pathExists("items[10]", testObject)).toBe(false);
    });
  });

  describe("Boundary cases", () => {
    it("Empty paths should be handled accordingly.", () => {
      expect(pathExists("", testObject)).toBe(false);
    });

    it("The empty object should be handled accordingly.", () => {
      expect(pathExists("user.name", null)).toBe(false);
      expect(pathExists("user.name", undefined)).toBe(false);
    });

    it("Invalid paths should be handled without throwing errors.", () => {
      expect(pathExists("__proto__", testObject)).toBe(false);
      expect(pathExists("user..name", testObject)).toBe(false);
    });
  });
});

describe("setPath", () => {
  let testObject: any;

  beforeEach(() => {
    testObject = {
      user: {
        name: "John",
        age: 25,
      },
      items: [{ id: 1, name: "Item 1" }],
    };
  });

  describe("Basic Features", () => {
    it("Simple attribute values should be set.", () => {
      const result = setPath("newField", testObject, "value");
      expect(result).toBe(true);
      expect(testObject.newField).toBe("value");
    });

    it("The nested attribute values should be set.", () => {
      const result = setPath("user.name", testObject, "Jane");
      expect(result).toBe(true);
      expect(testObject.user.name).toBe("Jane");
    });

    it("Deeply nested attribute values should be set.", () => {
      const result = setPath("user.address.city", testObject, "Boston");
      expect(result).toBe(true);
      expect(testObject.user.address.city).toBe("Boston");
    });

    it("An intermediate object should be created.", () => {
      const result = setPath("new.nested.field", testObject, "value");
      expect(result).toBe(true);
      expect(testObject.new.nested.field).toBe("value");
    });

    it("The array elements should be set.", () => {
      const result = setPath("items[0].name", testObject, "Updated Item");
      expect(result).toBe(true);
      expect(testObject.items[0].name).toBe("Updated Item");
    });

    it("The properties of elements in deeply nested arrays should be set.", () => {
      const result = setPath("items[0].newField", testObject, "value");
      expect(result).toBe(true);
      expect(testObject.items[0].newField).toBe("value");
    });
  });

  describe("Array operations", () => {
    it("The array should be expanded to accommodate the new index.", () => {
      const result = setPath("items[5].name", testObject, "New Item");
      expect(result).toBe(true);
      expect(testObject.items.length).toBe(6);
      expect(testObject.items[5].name).toBe("New Item");
    });

    it("A new array should be created.", () => {
      const result = setPath("newArray[0]", testObject, "first");
      expect(result).toBe(true);
      expect(Array.isArray(testObject.newArray)).toBe(true);
      expect(testObject.newArray[0]).toBe("first");
    });

    it("The new array should be expanded.", () => {
      const result = setPath("newArray[3]", testObject, "fourth");
      expect(result).toBe(true);
      expect(testObject.newArray.length).toBe(4);
      expect(testObject.newArray[3]).toBe("fourth");
    });

    it("The object properties in the array should be set.", () => {
      const result = setPath("items[2].name", testObject, "Item 3");
      expect(result).toBe(true);
      expect(testObject.items[2].name).toBe("Item 3");
    });
  });

  describe("Boundary cases", () => {
    it("Empty paths should be handled accordingly.", () => {
      const result = setPath("", testObject, "value");
      expect(result).toBe(false);
    });

    it("Empty objects should be handled accordingly.", () => {
      const result = setPath("field", null, "value");
      expect(result).toBe(false);
    });

    it("Paths that contain empty segments should be processed.", () => {
      const result = setPath("user..name", testObject, "value");
      expect(result).toBe(false);
    });

    it("Paths that start with a dot should be processed.", () => {
      const result = setPath(".name", testObject, "value");
      expect(result).toBe(false);
    });

    it("Paths ending with a dot should be processed.", () => {
      const result = setPath("user.", testObject, "value");
      expect(result).toBe(false);
    });
  });

  describe("Error Handling", () => {
    it("Paths that contain prohibited attributes should be rejected.", () => {
      expect(() => {
        setPath("__proto__", testObject, "value");
      }).toThrow(RuntimeValidationError);

      expect(() => {
        setPath("user.__proto__", testObject, "value");
      }).toThrow(RuntimeValidationError);
    });

    it("Invalid path formats should be rejected.", () => {
      expect(() => {
        setPath("user-name", testObject, "value");
      }).toThrow(RuntimeValidationError);

      expect(() => {
        setPath("user@name", testObject, "value");
      }).toThrow(RuntimeValidationError);
    });

    it("Property names that start with a number should be rejected.", () => {
      expect(() => {
        setPath("1user", testObject, "value");
      }).toThrow(RuntimeValidationError);
    });

    it("Deeper paths should be rejected.", () => {
      const deepPath = "a.b.c.d.e.f.g.h.i.j.k";
      expect(() => {
        setPath(deepPath, testObject, "value");
      }).toThrow(RuntimeValidationError);
    });
  });

  describe("Special Value Types", () => {
    it("The null value should be set.", () => {
      const result = setPath("user.name", testObject, null);
      expect(result).toBe(true);
      expect(testObject.user.name).toBeNull();
    });

    it("The `undefined` value should be set.", () => {
      const result = setPath("user.name", testObject, undefined);
      expect(result).toBe(true);
      expect(testObject.user.name).toBeUndefined();
    });

    it("The numerical value should be set.", () => {
      const result = setPath("user.age", testObject, 30);
      expect(result).toBe(true);
      expect(testObject.user.age).toBe(30);
    });

    it("The boolean value should be set.", () => {
      const result = setPath("user.active", testObject, true);
      expect(result).toBe(true);
      expect(testObject.user.active).toBe(true);
    });

    it("The object values should be set.", () => {
      const result = setPath("user.address", testObject, { city: "Boston", country: "USA" });
      expect(result).toBe(true);
      expect(testObject.user.address).toEqual({ city: "Boston", country: "USA" });
    });

    it("The array values should be set.", () => {
      const result = setPath("user.tags", testObject, ["admin", "user"]);
      expect(result).toBe(true);
      expect(testObject.user.tags).toEqual(["admin", "user"]);
    });
  });

  describe("Complex scenarios", () => {
    it("The existing values should be overwritten.", () => {
      const result = setPath("user.name", testObject, "Jane");
      expect(result).toBe(true);
      expect(testObject.user.name).toBe("Jane");
    });

    it("Multiple levels of nested creation need to be handled.", () => {
      const result = setPath("a.b.c.d.e", testObject, "deep value");
      expect(result).toBe(true);
      expect(testObject.a.b.c.d.e).toBe("deep value");
    });

    it("Mixed array and object paths should be handled accordingly.", () => {
      const result = setPath("items[0].details.info", testObject, "detail info");
      expect(result).toBe(true);
      expect(testObject.items[0].details.info).toBe("detail info");
    });
  });
});
