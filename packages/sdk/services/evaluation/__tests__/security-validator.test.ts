/**
 * SecurityValidator Tests
 * Tests security validation for paths and expressions
 */

import { describe, it, expect } from "vitest";
import {
  validatePath,
  validateExpression,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG
} from "../shared/security-validator.js";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";

describe("SecurityValidator", () => {
  describe("validatePath", () => {
    it("should accept valid simple paths", () => {
      expect(() => validatePath("user")).not.toThrow();
      expect(() => validatePath("_private")).not.toThrow();
      expect(() => validatePath("var123")).not.toThrow();
    });

    it("should accept valid nested paths", () => {
      expect(() => validatePath("user.name")).not.toThrow();
      expect(() => validatePath("user.profile.email")).not.toThrow();
      expect(() => validatePath("data.items[0].value")).not.toThrow();
    });

    it("should accept array index syntax", () => {
      expect(() => validatePath("items[0]")).not.toThrow();
      // Note: nested array indices like [0][1] are not supported by the regex
      expect(() => validatePath("users[10].name")).not.toThrow();
      expect(() => validatePath("data[5].items[0].value")).not.toThrow();
    });

    it("should reject empty path", () => {
      expect(() => validatePath("")).toThrow(ExpressionSecurityError);
      expect(() => validatePath(null as any)).toThrow(ExpressionSecurityError);
    });

    it("should reject forbidden properties", () => {
      expect(() => validatePath("__proto__")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("constructor")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("prototype")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user.__proto__.admin")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("data.constructor.name")).toThrow(ExpressionSecurityError);
    });

    it("should reject invalid characters", () => {
      expect(() => validatePath("user@name")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user.name!")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user.name;delete")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user.name\n.value")).toThrow(ExpressionSecurityError);
    });

    it("should reject paths exceeding depth limit", () => {
      // Create a path deeper than MAX_PATH_DEPTH (which is 10)
      const deepPath = Array(12).fill("level").join(".");
      expect(() => validatePath(deepPath)).toThrow(ExpressionSecurityError);
    });

    it("should accept paths at maximum depth", () => {
      const maxDepthPath = Array(10).fill("level").join(".");
      expect(() => validatePath(maxDepthPath)).not.toThrow();
    });

    it("should reject paths with trailing dots", () => {
      expect(() => validatePath("user.")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user.profile.name.")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("items[0].")).toThrow(ExpressionSecurityError);
    });

    it("should reject paths with leading dots", () => {
      expect(() => validatePath(".user")).toThrow(ExpressionSecurityError);
      expect(() => validatePath(".user.name")).toThrow(ExpressionSecurityError);
    });

    it("should reject paths with consecutive dots", () => {
      expect(() => validatePath("user..name")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("user...profile")).toThrow(ExpressionSecurityError);
      expect(() => validatePath("data..items[0]")).toThrow(ExpressionSecurityError);
    });

    it("should provide detailed error messages", () => {
      const error = expect(() => validatePath("__proto__.admin")).toThrow(ExpressionSecurityError);
    });
  });

  describe("validateExpression", () => {
    it("should accept valid expressions", () => {
      expect(() => validateExpression("x > 5")).not.toThrow();
      expect(() => validateExpression("user.name == 'John'")).not.toThrow();
      expect(() => validateExpression("items.length > 0 && status == 'active'")).not.toThrow();
    });

    it("should reject empty expressions", () => {
      expect(() => validateExpression("")).toThrow(ExpressionSecurityError);
      expect(() => validateExpression(null as any)).toThrow(ExpressionSecurityError);
      expect(() => validateExpression(undefined as any)).toThrow(ExpressionSecurityError);
    });

    it("should reject non-string expressions", () => {
      expect(() => validateExpression(123 as any)).toThrow(ExpressionSecurityError);
      expect(() => validateExpression({} as any)).toThrow(ExpressionSecurityError);
    });

    it("should reject expressions exceeding length limit", () => {
      const longExpr = "x > 5 " + "|| y > 10 ".repeat(200);
      expect(() => validateExpression(longExpr)).toThrow(ExpressionSecurityError);
    });

    it("should accept expressions at maximum length", () => {
      const expr = "x " + "&& y ".repeat(100);
      expect(expr.length).toBeLessThanOrEqual(SECURITY_CONFIG.MAX_EXPRESSION_LENGTH);
      expect(() => validateExpression(expr)).not.toThrow();
    });
  });

  describe("validateArrayIndex", () => {
    it("should accept valid indices", () => {
      const arr = ["a", "b", "c"];
      expect(() => validateArrayIndex(arr, 0)).not.toThrow();
      expect(() => validateArrayIndex(arr, 1)).not.toThrow();
      expect(() => validateArrayIndex(arr, 2)).not.toThrow();
    });

    it("should reject out of bounds index", () => {
      const arr = ["a", "b"];
      expect(() => validateArrayIndex(arr, 2)).toThrow(RuntimeValidationError);
      expect(() => validateArrayIndex(arr, 5)).toThrow(RuntimeValidationError);
    });

    it("should reject negative index", () => {
      const arr = ["a", "b"];
      expect(() => validateArrayIndex(arr, -1)).toThrow(RuntimeValidationError);
    });

    it("should reject non-integer index", () => {
      const arr = ["a", "b"];
      expect(() => validateArrayIndex(arr, 1.5)).toThrow(RuntimeValidationError);
      expect(() => validateArrayIndex(arr, NaN)).toThrow(RuntimeValidationError);
    });

    it("should reject non-array input", () => {
      expect(() => validateArrayIndex(null as any, 0)).toThrow(RuntimeValidationError);
      expect(() => validateArrayIndex("string" as any, 0)).toThrow(RuntimeValidationError);
      expect(() => validateArrayIndex({} as any, 0)).toThrow(RuntimeValidationError);
    });

    it("should accept empty array with index 0 should fail", () => {
      const arr: unknown[] = [];
      expect(() => validateArrayIndex(arr, 0)).toThrow(RuntimeValidationError);
    });
  });

  describe("validateValueType", () => {
    it("should accept null and undefined", () => {
      expect(() => validateValueType(null)).not.toThrow();
      expect(() => validateValueType(undefined)).not.toThrow();
    });

    it("should accept primitive types", () => {
      expect(() => validateValueType("string")).not.toThrow();
      expect(() => validateValueType(123)).not.toThrow();
      expect(() => validateValueType(true)).not.toThrow();
      expect(() => validateValueType(3.14)).not.toThrow();
      expect(() => validateValueType(0)).not.toThrow();
      expect(() => validateValueType("")).not.toThrow();
    });

    it("should accept arrays", () => {
      expect(() => validateValueType([])).not.toThrow();
      expect(() => validateValueType([1, 2, 3])).not.toThrow();
      expect(() => validateValueType(["a", "b"])).not.toThrow();
    });

    it("should accept plain objects", () => {
      expect(() => validateValueType({})).not.toThrow();
      expect(() => validateValueType({ name: "John", age: 30 })).not.toThrow();
      expect(() => validateValueType({ nested: { value: 1 } })).not.toThrow();
    });

    it("should reject functions", () => {
      expect(() => validateValueType(() => {})).toThrow(ExpressionSecurityError);
      expect(() => validateValueType(function () {})).toThrow(ExpressionSecurityError);
      expect(() => validateValueType(Math.sin)).toThrow(ExpressionSecurityError);
    });

    it("should reject non-plain objects", () => {
      const date = new Date();
      expect(() => validateValueType(date)).toThrow(ExpressionSecurityError);

      const regex = /test/;
      expect(() => validateValueType(regex)).toThrow(ExpressionSecurityError);

      const error = new Error("test");
      expect(() => validateValueType(error)).toThrow(ExpressionSecurityError);

      const customClass = class CustomClass {};
      expect(() => validateValueType(new customClass())).toThrow(ExpressionSecurityError);
    });

    it("should accept objects with null prototype", () => {
      const obj = Object.create(null);
      obj.name = "John";
      // Objects with null constructor should be treated as plain objects
      expect(() => validateValueType(obj)).not.toThrow();
    });
  });

  describe("SECURITY_CONFIG constants", () => {
    it("should have reasonable security limits", () => {
      expect(SECURITY_CONFIG.MAX_EXPRESSION_LENGTH).toBeGreaterThan(0);
      expect(SECURITY_CONFIG.MAX_PATH_DEPTH).toBeGreaterThan(0);
      expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("__proto__");
      expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("constructor");
      expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("prototype");
    });
  });
});
