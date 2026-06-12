import { describe, it, expect } from "vitest";
import {
  validateExpression,
  validatePath,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
} from "../security-validator.js";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";

describe("validateExpression", () => {
  it("should accept a valid expression", () => {
    expect(() => validateExpression("a + b")).not.toThrow();
  });

  it("should throw for empty string", () => {
    expect(() => validateExpression("")).toThrow(ExpressionSecurityError);
  });

  it("should throw for non-string input", () => {
    expect(() => validateExpression(null as any)).toThrow(ExpressionSecurityError);
    expect(() => validateExpression(undefined as any)).toThrow(ExpressionSecurityError);
    expect(() => validateExpression(42 as any)).toThrow(ExpressionSecurityError);
  });

  it("should throw for expression exceeding max length", () => {
    const longExpr = "a".repeat(SECURITY_CONFIG.MAX_EXPRESSION_LENGTH + 1);
    expect(() => validateExpression(longExpr)).toThrow(ExpressionSecurityError);
  });

  it("should accept expression at max length boundary", () => {
    const expr = "a".repeat(SECURITY_CONFIG.MAX_EXPRESSION_LENGTH);
    expect(() => validateExpression(expr)).not.toThrow();
  });
});

describe("validatePath", () => {
  it("should accept a simple path", () => {
    expect(() => validatePath("user")).not.toThrow();
  });

  it("should accept a nested path", () => {
    expect(() => validatePath("user.name")).not.toThrow();
  });

  it("should accept a path with array index", () => {
    expect(() => validatePath("items[0]")).not.toThrow();
    expect(() => validatePath("items[0].name")).not.toThrow();
    expect(() => validatePath("data.items[0].name")).not.toThrow();
  });

  it("should throw for empty string path", () => {
    expect(() => validatePath("")).toThrow(ExpressionSecurityError);
  });

  it("should throw for non-string path", () => {
    expect(() => validatePath(null as any)).toThrow(ExpressionSecurityError);
    expect(() => validatePath(undefined as any)).toThrow(ExpressionSecurityError);
  });

  it("should throw for forbidden properties", () => {
    expect(() => validatePath("__proto__")).toThrow(ExpressionSecurityError);
    expect(() => validatePath("obj.constructor")).toThrow(ExpressionSecurityError);
    expect(() => validatePath("obj.prototype")).toThrow(ExpressionSecurityError);
  });

  it("should throw for invalid path characters", () => {
    expect(() => validatePath("user@name")).toThrow(ExpressionSecurityError);
    expect(() => validatePath("user name")).toThrow(ExpressionSecurityError);
    expect(() => validatePath("user-name")).toThrow(ExpressionSecurityError);
  });

  it("should throw for path starting with number", () => {
    expect(() => validatePath("1user")).toThrow(ExpressionSecurityError);
  });

  it("should throw for empty path parts", () => {
    expect(() => validatePath("user..name")).toThrow(ExpressionSecurityError);
  });

  it("should throw for path exceeding max depth", () => {
    const deepPath = Array.from(
      { length: SECURITY_CONFIG.MAX_PATH_DEPTH + 2 },
      (_, i) => `p${i}`,
    ).join(".");
    expect(() => validatePath(deepPath)).toThrow(ExpressionSecurityError);
  });

  it("should accept path at max depth boundary", () => {
    const path = Array.from({ length: SECURITY_CONFIG.MAX_PATH_DEPTH }, (_, i) => `p${i}`).join(
      ".",
    );
    expect(() => validatePath(path)).not.toThrow();
  });
});

describe("validateArrayIndex", () => {
  it("should accept a valid index", () => {
    expect(() => validateArrayIndex([1, 2, 3], 1)).not.toThrow();
  });

  it("should accept first and last index", () => {
    expect(() => validateArrayIndex([1, 2, 3], 0)).not.toThrow();
    expect(() => validateArrayIndex([1, 2, 3], 2)).not.toThrow();
  });

  it("should throw for non-array target", () => {
    expect(() => validateArrayIndex(undefined as any, 0)).toThrow(RuntimeValidationError);
    expect(() => validateArrayIndex("not-array" as any, 0)).toThrow(RuntimeValidationError);
  });

  it("should throw for negative index", () => {
    expect(() => validateArrayIndex([1, 2, 3], -1)).toThrow(RuntimeValidationError);
  });

  it("should throw for index out of bounds", () => {
    expect(() => validateArrayIndex([1, 2, 3], 5)).toThrow(RuntimeValidationError);
    expect(() => validateArrayIndex([1, 2, 3], 3)).toThrow(RuntimeValidationError);
  });

  it("should throw for non-integer index", () => {
    expect(() => validateArrayIndex([1, 2, 3], 1.5)).toThrow(RuntimeValidationError);
  });
});

describe("validateValueType", () => {
  it("should accept null and undefined", () => {
    expect(() => validateValueType(null)).not.toThrow();
    expect(() => validateValueType(undefined)).not.toThrow();
  });

  it("should accept primitive types", () => {
    expect(() => validateValueType("string")).not.toThrow();
    expect(() => validateValueType(42)).not.toThrow();
    expect(() => validateValueType(true)).not.toThrow();
  });

  it("should accept arrays", () => {
    expect(() => validateValueType([1, 2, 3])).not.toThrow();
    expect(() => validateValueType([])).not.toThrow();
  });

  it("should accept plain objects", () => {
    expect(() => validateValueType({})).not.toThrow();
    expect(() => validateValueType({ a: 1 })).not.toThrow();
  });

  it("should reject functions", () => {
    expect(() => validateValueType(() => {})).toThrow(ExpressionSecurityError);
  });

  it("should reject class instances (non-plain objects)", () => {
    class CustomClass {}
    expect(() => validateValueType(new CustomClass())).toThrow(ExpressionSecurityError);
  });
});

describe("SECURITY_CONFIG", () => {
  it("should have expected configuration values", () => {
    expect(SECURITY_CONFIG.MAX_EXPRESSION_LENGTH).toBe(1000);
    expect(SECURITY_CONFIG.MAX_PATH_DEPTH).toBe(10);
    expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("__proto__");
    expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("constructor");
    expect(SECURITY_CONFIG.FORBIDDEN_PROPERTIES).toContain("prototype");
  });
});
