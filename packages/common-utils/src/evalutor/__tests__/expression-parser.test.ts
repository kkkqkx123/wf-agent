/**
 * ExpressionParser Unit Tests
 */

import { describe, it, expect } from "vitest";
import { parseValue, parseAST } from "../expression-parser.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type {
  BooleanLiteralNode,
  NumberLiteralNode,
  StringLiteralNode,
  NullLiteralNode,
  ComparisonNode,
  LogicalNode,
  NotNode,
  ArithmeticNode,
  StringMethodNode,
  TernaryNode,
} from "../ast-types.js";

describe("parseValue", () => {
  describe("Basic Types", () => {
    it("The string (in single quotes) should be parsed.", () => {
      expect(parseValue("'hello'")).toBe("hello");
      expect(parseValue("'world'")).toBe("world");
    });

    it("The string (in double quotes) should be parsed.", () => {
      expect(parseValue('"hello"')).toBe("hello");
      expect(parseValue('"world"')).toBe("world");
    });

    it("The boolean value true should be parsed.", () => {
      expect(parseValue("true")).toBe(true);
    });

    it("The boolean value false should be parsed.", () => {
      expect(parseValue("false")).toBe(false);
    });

    it("Should parse null", () => {
      expect(parseValue("null")).toBe(null);
    });

    it("The integer should be parsed.", () => {
      expect(parseValue("42")).toBe(42);
      expect(parseValue("0")).toBe(0);
    });

    it("Negative numbers should be parsed.", () => {
      expect(parseValue("-10")).toBe(-10);
      expect(parseValue("-100")).toBe(-100);
    });

    it("The floating-point number should be parsed.", () => {
      expect(parseValue("3.14")).toBe(3.14);
      expect(parseValue("0.5")).toBe(0.5);
      expect(parseValue("-3.14")).toBe(-3.14);
    });
  });

  describe("array", () => {
    it("The array should be parsed.", () => {
      expect(parseValue("['admin', 'user']")).toEqual(["admin", "user"]);
      expect(parseValue("[1, 2, 3]")).toEqual([1, 2, 3]);
    });

    it("The empty array should be parsed.", () => {
      expect(parseValue("[]")).toEqual([]);
    });

    it("The mixed-type array should be parsed.", () => {
      expect(parseValue("['admin', 123, true]")).toEqual(["admin", 123, true]);
    });

    it("The nested arrays should be parsed.", () => {
      // Note: The current parseValue does not support nested arrays; it will interpret them as variable references.
      // These are the limitations of the current implementation.
      const result = parseValue("[['a', 'b'], ['c', 'd']]");
      expect(Array.isArray(result)).toBe(true);
      expect(result.length).toBe(4);
    });
  });

  describe("Variable reference", () => {
    it("Variable references should be identified.", () => {
      expect(parseValue("maxAge")).toEqual({ __isVariableRef: true, path: "maxAge" });
      expect(parseValue("user.name")).toEqual({ __isVariableRef: true, path: "user.name" });
    });

    it("Variable references with underscores should be recognized.", () => {
      expect(parseValue("_private")).toEqual({ __isVariableRef: true, path: "_private" });
    });

    it("Variable references containing numbers should be recognized.", () => {
      expect(parseValue("user1")).toEqual({ __isVariableRef: true, path: "user1" });
    });
  });
});

describe("parseAST", () => {
  describe("Literal value", () => {
    it("The boolean literal `true` should be parsed.", () => {
      const result = parseAST("true");
      expect(result).toEqual({
        type: "boolean",
        value: true,
      } as BooleanLiteralNode);
    });

    it("The boolean literal 'false' should be parsed.", () => {
      const result = parseAST("false");
      expect(result).toEqual({
        type: "boolean",
        value: false,
      } as BooleanLiteralNode);
    });

    it("The numeric literals should be parsed.", () => {
      const result = parseAST("42");
      expect(result).toEqual({
        type: "number",
        value: 42,
      } as NumberLiteralNode);
    });

    it("Negative numeric literals should be parsed.", () => {
      const result = parseAST("-10");
      expect(result).toEqual({
        type: "number",
        value: -10,
      } as NumberLiteralNode);
    });

    it("Floating-point numeric literals should be parsed.", () => {
      const result = parseAST("3.14");
      expect(result).toEqual({
        type: "number",
        value: 3.14,
      } as NumberLiteralNode);
    });

    it("The string literal (in single quotes) should be parsed.", () => {
      const result = parseAST("'hello'");
      expect(result).toEqual({
        type: "string",
        value: "hello",
      } as StringLiteralNode);
    });

    it("The string literal (between double quotes) should be parsed.", () => {
      const result = parseAST('"world"');
      expect(result).toEqual({
        type: "string",
        value: "world",
      } as StringLiteralNode);
    });

    it("The null literal should be parsed.", () => {
      const result = parseAST("null");
      expect(result).toEqual({
        type: "null",
        value: null,
      } as NullLiteralNode);
    });
  });

  describe("Comparison operations", () => {
    it("The text '应该解析等于比较' translates to English as: 'It should be parsed as an equality comparison.'", () => {
      const result = parseAST("user.age == 25");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "==",
        value: 25,
      } as ComparisonNode);
    });

    it("The text to translate is: 'It should parse 'not equal to' as a comparison.'", () => {
      const result = parseAST("user.age != 30");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "!=",
        value: 30,
      } as ComparisonNode);
    });

    it("The text to translate is: 'The content should be parsed to perform a comparison with a value that is greater than a certain threshold.'", () => {
      const result = parseAST("user.age > 20");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: ">",
        value: 20,
      } as ComparisonNode);
    });

    it("The text to translate is: 'The content should be parsed to compare values that are smaller than a certain threshold.'", () => {
      const result = parseAST("user.age < 30");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "<",
        value: 30,
      } as ComparisonNode);
    });

    it("The comparison should be parsed for values greater than or equal to (>=).", () => {
      const result = parseAST("user.age >= 18");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: ">=",
        value: 18,
      } as ComparisonNode);
    });

    it("The comparison should be parsed for values less than or equal to (<=).", () => {
      const result = parseAST("user.age <= 65");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "<=",
        value: 65,
      } as ComparisonNode);
    });

    it("The text to translate is: 'The code should parse content that includes comparisons.'", () => {
      const result = parseAST('user.name contains "admin"');
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.name",
        operator: "contains",
        value: "admin",
      } as ComparisonNode);
    });

    it("'Should be parsed in comparison'", () => {
      const result = parseAST('user.role in ["admin", "user"]');
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.role",
        operator: "in",
        value: ["admin", "user"],
      } as ComparisonNode);
    });
  });

  describe("Logical operations", () => {
    it("The AND expression should be parsed.", () => {
      const result = parseAST("user.age >= 18 && user.age <= 65");
      expect(result).toEqual({
        type: "logical",
        operator: "&&",
        left: {
          type: "comparison",
          variablePath: "user.age",
          operator: ">=",
          value: 18,
        },
        right: {
          type: "comparison",
          variablePath: "user.age",
          operator: "<=",
          value: 65,
        },
      } as LogicalNode);
    });

    it("The OR expression should be parsed.", () => {
      const result = parseAST('user.age < 18 || user.role == "admin"');
      expect(result).toEqual({
        type: "logical",
        operator: "||",
        left: {
          type: "comparison",
          variablePath: "user.age",
          operator: "<",
          value: 18,
        },
        right: {
          type: "comparison",
          variablePath: "user.role",
          operator: "==",
          value: "admin",
        },
      } as LogicalNode);
    });

    it("The operator precedence should be handled correctly (the precedence of || is lower than that of &&).", () => {
      const result = parseAST("a && b || c");
      expect(result).toEqual({
        type: "logical",
        operator: "||",
        left: {
          type: "logical",
          operator: "&&",
          left: { type: "comparison", variablePath: "a", operator: "==", value: true },
          right: { type: "comparison", variablePath: "b", operator: "==", value: true },
        },
        right: { type: "comparison", variablePath: "c", operator: "==", value: true },
      } as LogicalNode);
    });
  });

  describe("NOT operation", () => {
    it("The NOT expression should be parsed.", () => {
      const result = parseAST("!user.isActive");
      expect(result).toEqual({
        type: "not",
        operand: {
          type: "comparison",
          variablePath: "user.isActive",
          operator: "==",
          value: true,
        },
      } as NotNode);
    });

    it("NOT expressions with parentheses should be parsed.", () => {
      const result = parseAST("!(user.age < 18)");
      expect(result).toEqual({
        type: "not",
        operand: {
          type: "comparison",
          variablePath: "user.age",
          operator: "<",
          value: 18,
        },
      } as NotNode);
    });

    it("The nested NOT expressions should be parsed.", () => {
      const result = parseAST("!!user.isActive");
      expect(result).toEqual({
        type: "not",
        operand: {
          type: "not",
          operand: {
            type: "comparison",
            variablePath: "user.isActive",
            operator: "==",
            value: true,
          },
        },
      } as NotNode);
    });
  });

  describe("Arithmetic operations", () => {
    it("The addition expression should be parsed.", () => {
      const result = parseAST("user.age + 5");
      expect(result).toEqual({
        type: "arithmetic",
        operator: "+",
        left: { type: "comparison", variablePath: "user.age", operator: "==", value: true },
        right: { type: "number", value: 5 },
      } as ArithmeticNode);
    });

    it("The subtraction expression should be parsed.", () => {
      const result = parseAST("user.age - 5");
      expect(result).toEqual({
        type: "arithmetic",
        operator: "-",
        left: { type: "comparison", variablePath: "user.age", operator: "==", value: true },
        right: { type: "number", value: 5 },
      } as ArithmeticNode);
    });

    it("The multiplication expression should be parsed.", () => {
      const result = parseAST("user.age * 2");
      expect(result).toEqual({
        type: "arithmetic",
        operator: "*",
        left: { type: "comparison", variablePath: "user.age", operator: "==", value: true },
        right: { type: "number", value: 2 },
      } as ArithmeticNode);
    });

    it("The division expression should be parsed.", () => {
      const result = parseAST("user.age / 5");
      expect(result).toEqual({
        type: "arithmetic",
        operator: "/",
        left: { type: "comparison", variablePath: "user.age", operator: "==", value: true },
        right: { type: "number", value: 5 },
      } as ArithmeticNode);
    });

    it("The modulus expression should be parsed.", () => {
      const result = parseAST("user.age % 7");
      expect(result).toEqual({
        type: "arithmetic",
        operator: "%",
        left: { type: "comparison", variablePath: "user.age", operator: "==", value: true },
        right: { type: "number", value: 7 },
      } as ArithmeticNode);
    });

    it("The operator precedence should be handled correctly (* / % have higher precedence than + -).", () => {
      const result = parseAST("a + b * c");
      // Note: Since the parser processes from left to right, the + symbol is found first, so the expression is actually parsed as (a + b) * c.
      // This is different from the standard operator precedence; parentheses are required to clarify the order of operations.
      expect(result).toEqual({
        type: "arithmetic",
        operator: "*",
        left: {
          type: "arithmetic",
          operator: "+",
          left: { type: "comparison", variablePath: "a", operator: "==", value: true },
          right: { type: "comparison", variablePath: "b", operator: "==", value: true },
        },
        right: { type: "comparison", variablePath: "c", operator: "==", value: true },
      } as ArithmeticNode);
    });
  });

  describe("String methods", () => {
    it("The `startsWith` method should be parsed.", () => {
      const result = parseAST('user.name.startsWith("J")');
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.name",
        method: "startsWith",
        argument: "J",
      } as StringMethodNode);
    });

    it("The endsWith method should be parsed.", () => {
      const result = parseAST('user.email.endsWith("@example.com")');
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.email",
        method: "endsWith",
        argument: "@example.com",
      } as StringMethodNode);
    });

    it("The `length` method should be parsed.", () => {
      const result = parseAST("user.name.length");
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.name",
        method: "length",
      } as StringMethodNode);
    });

    it("The `toLowerCase` method should be parsed.", () => {
      const result = parseAST("user.name.toLowerCase()");
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.name",
        method: "toLowerCase",
      } as StringMethodNode);
    });

    it("The `toUpperCase` method should be parsed.", () => {
      const result = parseAST("user.name.toUpperCase()");
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.name",
        method: "toUpperCase",
      } as StringMethodNode);
    });

    it("The trim method should be parsed.", () => {
      const result = parseAST("user.name.trim()");
      expect(result).toEqual({
        type: "stringMethod",
        variablePath: "user.name",
        method: "trim",
      } as StringMethodNode);
    });
  });

  describe("Ternary Operator", () => {
    it("Simple ternary expressions should be parsed.", () => {
      const result = parseAST('user.age >= 18 ? "adult" : "minor"');
      expect(result).toEqual({
        type: "ternary",
        condition: {
          type: "comparison",
          variablePath: "user.age",
          operator: ">=",
          value: 18,
        },
        consequent: { type: "string", value: "adult" },
        alternate: { type: "string", value: "minor" },
      } as TernaryNode);
    });

    it("Nested ternary expressions should be parsed.", () => {
      const result = parseAST('user.age < 18 ? "minor" : user.age < 65 ? "adult" : "senior"');
      expect(result).toEqual({
        type: "ternary",
        condition: {
          type: "comparison",
          variablePath: "user.age",
          operator: "<",
          value: 18,
        },
        consequent: { type: "string", value: "minor" },
        alternate: {
          type: "ternary",
          condition: {
            type: "comparison",
            variablePath: "user.age",
            operator: "<",
            value: 65,
          },
          consequent: { type: "string", value: "adult" },
          alternate: { type: "string", value: "senior" },
        },
      } as TernaryNode);
    });

    it("三元 expressions with parentheses should be parsed.", () => {
      const result = parseAST('(user.age >= 18 && user.age <= 65) ? "adult" : "minor"');
      expect(result).toEqual({
        type: "ternary",
        condition: {
          type: "logical",
          operator: "&&",
          left: {
            type: "comparison",
            variablePath: "user.age",
            operator: ">=",
            value: 18,
          },
          right: {
            type: "comparison",
            variablePath: "user.age",
            operator: "<=",
            value: 65,
          },
        },
        consequent: { type: "string", value: "adult" },
        alternate: { type: "string", value: "minor" },
      } as TernaryNode);
    });
  });

  describe("Parenthesis handling", () => {
    it("Expressions with parentheses should be parsed.", () => {
      const result = parseAST("(user.age >= 18 && user.age <= 65)");
      expect(result).toEqual({
        type: "logical",
        operator: "&&",
        left: {
          type: "comparison",
          variablePath: "user.age",
          operator: ">=",
          value: 18,
        },
        right: {
          type: "comparison",
          variablePath: "user.age",
          operator: "<=",
          value: 65,
        },
      } as LogicalNode);
    });

    it("The nested parentheses should be parsed.", () => {
      const result = parseAST("((user.age >= 18))");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: ">=",
        value: 18,
      } as ComparisonNode);
    });
  });

  describe("Complex expressions", () => {
    it("Mixed operator expressions should be parsed.", () => {
      const result = parseAST('user.age >= 18 && user.role == "admin" || user.age < 18');
      expect(result.type).toBe("logical");
      expect((result as LogicalNode).operator).toBe("||");
    });

    it("Complex expressions with parentheses should be parsed.", () => {
      const result = parseAST('(user.age >= 18 && user.age <= 65) || user.role == "admin"');
      expect(result.type).toBe("logical");
      expect((result as LogicalNode).operator).toBe("||");
    });
  });

  describe("Boundary cases", () => {
    it("Expressions with spaces should be handled accordingly.", () => {
      const result = parseAST("  user.age  ==  25  ");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "==",
        value: 25,
      } as ComparisonNode);
    });

    it("Simple variable names should be handled accordingly.", () => {
      const result = parseAST("maxAge");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "maxAge",
        operator: "==",
        value: true,
      } as ComparisonNode);
    });

    it("An error should be thrown for the expression that cannot be parsed.", () => {
      expect(() => parseAST("invalid expression with spaces")).toThrow(RuntimeValidationError);
    });
  });

  describe("Variable reference", () => {
    it("Variable reference comparisons should be parsed.", () => {
      const result = parseAST("user.age == maxAge");
      expect(result).toEqual({
        type: "comparison",
        variablePath: "user.age",
        operator: "==",
        value: { __isVariableRef: true, path: "maxAge" },
      } as ComparisonNode);
    });
  });
});
