/**
 * DSL Parser Unit Tests
 */

import { describe, it, expect } from "vitest";
import { dslParse, dslValidate, tokenizeExpression } from "../dsl/index.js";

describe("dslParse", () => {
  describe("Basic literals", () => {
    it("should parse string literals in single quotes", () => {
      const ast = dslParse("'hello'") as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("literal");
      expect(ast.valueType).toBe("string");
      expect(ast.value).toBe("hello");
    });

    it("should parse string literals in double quotes", () => {
      const ast = dslParse('"world"') as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("literal");
      expect(ast.valueType).toBe("string");
      expect(ast.value).toBe("world");
    });

    it("should parse number literals", () => {
      const ast = dslParse("42") as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("literal");
      expect(ast.valueType).toBe("number");
      expect(ast.value).toBe(42);
    });

    it("should parse negative number literals", () => {
      const ast = dslParse("-10") as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("unaryMinus");
    });

    it("should parse floating point numbers", () => {
      const ast = dslParse("3.14") as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("literal");
      expect(ast.valueType).toBe("number");
      expect(ast.value).toBe(3.14);
    });

    it("should parse boolean literals", () => {
      const t = dslParse("true") as any;
      expect(t).not.toBeNull();
      expect(t.type).toBe("literal");
      expect(t.valueType).toBe("boolean");
      expect(t.value).toBe(true);

      const f = dslParse("false") as any;
      expect(f).not.toBeNull();
      expect(f.type).toBe("literal");
      expect(f.valueType).toBe("boolean");
      expect(f.value).toBe(false);
    });

    it("should parse null literal", () => {
      const ast = dslParse("null") as any;
      expect(ast).not.toBeNull();
      expect(ast.type).toBe("literal");
      expect(ast.valueType).toBe("null");
      expect(ast.value).toBeNull();
    });
  });

  describe("Identifiers and member access", () => {
    it("should parse simple identifiers", () => {
      const ast = dslParse("user");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("identifier");
      expect((ast! as any).name).toBe("user");
    });

    it("should parse member access with dot", () => {
      const ast = dslParse("user.age");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("memberAccess");
      expect((ast! as any).property).toBe("age");
    });

    it("should parse deeply nested member access", () => {
      const ast = dslParse("user.address.city");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("memberAccess");
      const member = ast as any;
      expect(member.property).toBe("city");
      expect(member.object.type).toBe("memberAccess");
      expect(member.object.property).toBe("address");
    });
  });

  describe("Comparison operators", () => {
    it("should parse == operator", () => {
      const ast = dslParse("user.age == 25");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("==");
    });

    it("should parse != operator", () => {
      const ast = dslParse("user.age != 30");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("!=");
    });

    it("should parse > operator", () => {
      const ast = dslParse("user.age > 18");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe(">");
    });

    it("should parse < operator", () => {
      const ast = dslParse("user.age < 65");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("<");
    });

    it("should parse >= operator", () => {
      const ast = dslParse("user.age >= 18");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe(">=");
    });

    it("should parse <= operator", () => {
      const ast = dslParse("user.age <= 65");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("<=");
    });

    it("should parse contains operator", () => {
      const ast = dslParse("user.name contains 'oh'");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("contains");
    });

    it("should parse in operator", () => {
      const ast = dslParse("user.role in ['admin', 'user']");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("in");
    });
  });

  describe("Logical operators", () => {
    it("should parse && operator", () => {
      const ast = dslParse("user.age > 18 && user.active == true");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("&&");
    });

    it("should parse || operator", () => {
      const ast = dslParse("user.age < 18 || user.role == 'admin'");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("||");
    });

    it("should parse NOT operator", () => {
      const ast = dslParse("!isActive");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("not");
    });
  });

  describe("Arithmetic operators", () => {
    it("should parse addition", () => {
      const ast = dslParse("25 + 5");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("+");
    });

    it("should parse subtraction", () => {
      const ast = dslParse("25 - 5");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("-");
    });

    it("should parse multiplication", () => {
      const ast = dslParse("25 * 2");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("*");
    });

    it("should parse division", () => {
      const ast = dslParse("25 / 5");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("/");
    });

    it("should parse modulus", () => {
      const ast = dslParse("25 % 7");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
      expect((ast! as any).operator).toBe("%");
    });
  });

  describe("Ternary operator", () => {
    it("should parse ternary expression", () => {
      const ast = dslParse("user.age >= 18 ? 'adult' : 'minor'");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("ternary");
    });
  });

  describe("Function/method calls", () => {
    it("should parse method calls", () => {
      const ast = dslParse("text.startsWith('Hello')");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("call");
    });

    it("should parse method calls with no arguments", () => {
      const ast = dslParse("text.toLowerCase()");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("call");
    });

    it("should parse property access (length)", () => {
      const ast = dslParse("text.length");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("memberAccess");
    });
  });

  describe("Array literals", () => {
    it("should parse array literals", () => {
      const ast = dslParse("['admin', 'user']");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("arrayLiteral");
      expect((ast! as any).elements.length).toBe(2);
    });
  });

  describe("Parenthesized expressions", () => {
    it("should parse parenthesized expressions", () => {
      const ast = dslParse("(25 + 5) * 2");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");
    });
  });

  describe("dslValidate", () => {
    it("should return valid for correct expressions", () => {
      const result = dslValidate("user.age > 18");
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("should return invalid for incorrect expressions", () => {
      const result = dslValidate("user.age > ");
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe("tokenizeExpression", () => {
    it("should return an array of tokens", () => {
      const tokens = tokenizeExpression("user.age > 18");
      expect(tokens.length).toBeGreaterThan(0);
    });
  });
});