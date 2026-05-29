import { describe, it, expect } from "vitest";
import { conditionLexer } from "../tokens.js";
import { ConditionLexer, conditionLexerInstance } from "../condition-lexer.js";
import { conditionParser } from "../condition-parser.js";
import { conditionCstToAstVisitor } from "../condition-cst-to-ast.js";
import {
  dslParse,
  dslParseWithErrors,
  dslValidate,
  parseToCst,
  cstToAst,
  tokenizeExpression,
} from "../index.js";
import type { Expression } from "../types.js";

describe("Tokens", () => {
  it("should tokenize keywords correctly", () => {
    const result = conditionLexer.tokenize("true false null contains in");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["true", "false", "null", "contains", "in"]);
  });

  it("should tokenize comparison operators correctly", () => {
    const result = conditionLexer.tokenize("== != >= <= > <");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["==", "!=", ">=", "<=", ">", "<"]);
  });

  it("should tokenize logical operators correctly", () => {
    const result = conditionLexer.tokenize("&& || !");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["&&", "||", "!"]);
  });

  it("should tokenize arithmetic operators correctly", () => {
    const result = conditionLexer.tokenize("+ - * / %");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["+", "-", "*", "/", "%"]);
  });

  it("should tokenize string literals with single quotes", () => {
    const result = conditionLexer.tokenize("'hello world'");
    expect(result.errors).toHaveLength(0);
    expect(result.tokens[0]!.image).toBe("'hello world'");
  });

  it("should tokenize string literals with double quotes", () => {
    const result = conditionLexer.tokenize('"hello world"');
    expect(result.errors).toHaveLength(0);
    expect(result.tokens[0]!.image).toBe('"hello world"');
  });

  it("should tokenize string literals with escape sequences", () => {
    const result = conditionLexer.tokenize("'hello\\'world'");
    expect(result.errors).toHaveLength(0);
    expect(result.tokens[0]!.image).toBe("'hello\\'world'");
  });

  it("should tokenize number literals correctly", () => {
    const result = conditionLexer.tokenize("42 3.14");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["42", "3.14"]);
  });

  it("should tokenize identifiers correctly", () => {
    const result = conditionLexer.tokenize("foo bar _baz abc123");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["foo", "bar", "_baz", "abc123"]);
  });

  it("should distinguish keywords from identifiers", () => {
    const result = conditionLexer.tokenize("true trueValue");
    expect(result.errors).toHaveLength(0);
    expect(result.tokens[0]!.tokenType.name).toBe("True");
    expect(result.tokens[1]!.tokenType.name).toBe("Identifier");
  });

  it("should handle array methods", () => {
    const result = conditionLexer.tokenize("someEqual hasContains distinct");
    expect(result.errors).toHaveLength(0);
    result.tokens.forEach(t => {
      expect(t.tokenType.name).toBe("ArrayMethod");
    });
  });

  it("should handle string methods", () => {
    const result = conditionLexer.tokenize("startsWith toLowerCase trim");
    expect(result.errors).toHaveLength(0);
    result.tokens.forEach(t => {
      expect(t.tokenType.name).toBe("StringMethod");
    });
  });

  it("should skip whitespace", () => {
    const result = conditionLexer.tokenize("  a  +  b  ");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["a", "+", "b"]);
  });

  it("should skip line comments", () => {
    const result = conditionLexer.tokenize("a // this is a comment\n+ b");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["a", "+", "b"]);
  });

  it("should skip block comments", () => {
    const result = conditionLexer.tokenize("a /* comment */ + b");
    expect(result.errors).toHaveLength(0);
    const images = result.tokens.map(t => t.image);
    expect(images).toEqual(["a", "+", "b"]);
  });

  it("should produce errors for invalid characters", () => {
    const result = conditionLexer.tokenize("a @ b");
    expect(result.errors.length).toBeGreaterThan(0);
  });
});

describe("ConditionLexer", () => {
  const lexer = new ConditionLexer();

  it("should tokenize expression", () => {
    const result = lexer.tokenize("a == b");
    expect(result.tokens).toHaveLength(3);
  });

  it("should detect errors", () => {
    const result = lexer.tokenize("a @ b");
    expect(lexer.hasErrors(result)).toBe(true);
  });

  it("should report no errors for valid input", () => {
    const result = lexer.tokenize("a + b");
    expect(lexer.hasErrors(result)).toBe(false);
  });

  it("should format errors correctly", () => {
    const result = lexer.tokenize("@");
    const messages = lexer.formatErrors(result);
    expect(messages.length).toBeGreaterThan(0);
    expect(messages[0]).toContain("Lexer error");
  });

  it("getTokens should return tokens for valid input", () => {
    const tokens = lexer.getTokens("a + b");
    expect(tokens).toHaveLength(3);
  });

  it("getTokens should throw for invalid input", () => {
    expect(() => lexer.getTokens("@")).toThrow("Lexer errors");
  });

  it("singleton instance should work", () => {
    const result = conditionLexerInstance.tokenize("true");
    expect(result.tokens).toHaveLength(1);
  });
});

describe("ConditionParser", () => {
  it("should parse a simple identifier expression", () => {
    const result = conditionParser.parseExpression("foo");
    expect(result.cst).toBeDefined();
    expect(result.parseErrors).toHaveLength(0);
    expect(result.lexErrors).toHaveLength(0);
    expect(result.cst!.name).toBe("expression");
  });

  it("should parse a number literal", () => {
    const result = conditionParser.parseExpression("42");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse a string literal", () => {
    const result = conditionParser.parseExpression("'hello'");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse boolean literals", () => {
    const result1 = conditionParser.parseExpression("true");
    expect(result1.parseErrors).toHaveLength(0);

    const result2 = conditionParser.parseExpression("false");
    expect(result2.parseErrors).toHaveLength(0);
  });

  it("should parse null literal", () => {
    const result = conditionParser.parseExpression("null");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse arithmetic expression with precedence", () => {
    const result = conditionParser.parseExpression("1 + 2 * 3");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse comparison expression", () => {
    const result = conditionParser.parseExpression("a == b");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse logical expression", () => {
    const result = conditionParser.parseExpression("a && b || c");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse NOT expression", () => {
    const result = conditionParser.parseExpression("!a");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse double NOT expression", () => {
    const result = conditionParser.parseExpression("!!a");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse ternary expression", () => {
    const result = conditionParser.parseExpression("a ? b : c");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse nested ternary expression", () => {
    const result = conditionParser.parseExpression("a ? b ? c : d : e");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse parenthesized expression", () => {
    const result = conditionParser.parseExpression("(a + b) * c");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse unary minus expression", () => {
    const result = conditionParser.parseExpression("-42");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse contains expression", () => {
    const result = conditionParser.parseExpression("arr contains 5");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse 'in' expression", () => {
    const result = conditionParser.parseExpression("5 in arr");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse member access with dot", () => {
    const result = conditionParser.parseExpression("obj.property");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse chained member access", () => {
    const result = conditionParser.parseExpression("obj.prop1.prop2");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse method call", () => {
    const result = conditionParser.parseExpression("arr.someEqual(5)");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse method call with multiple arguments", () => {
    const result = conditionParser.parseExpression("arr.someEqual(5, 10)");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse string method call", () => {
    const result = conditionParser.parseExpression("str.startsWith('hello')");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse chained method calls", () => {
    const result = conditionParser.parseExpression("arr.map(x).distinct()");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse function call", () => {
    const result = conditionParser.parseExpression("myFunc(a, b)");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse array literal", () => {
    const result = conditionParser.parseExpression("[1, 2, 3]");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse empty array literal", () => {
    const result = conditionParser.parseExpression("[]");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse property access with array method name", () => {
    const result = conditionParser.parseExpression("obj.first");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse subscript access with number", () => {
    const result = conditionParser.parseExpression("arr[0]");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse subscript access with string", () => {
    const result = conditionParser.parseExpression("obj['key']");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should parse subscript access with identifier", () => {
    const result = conditionParser.parseExpression("obj[key]");
    expect(result.parseErrors).toHaveLength(0);
  });

  it("should produce parse errors for invalid syntax", () => {
    const result = conditionParser.parseExpression("a +");
    expect(result.parseErrors.length).toBeGreaterThan(0);
  });

  it("should produce lex errors for invalid tokens", () => {
    const result = conditionParser.parseExpression("a @ b");
    expect(result.lexErrors.length).toBeGreaterThan(0);
  });
});

describe("ConditionCstToAstVisitor", () => {
  const parseAndVisit = (expression: string): Expression => {
    const { cst, parseErrors, lexErrors } = conditionParser.parseExpression(expression);
    expect(parseErrors).toHaveLength(0);
    expect(lexErrors).toHaveLength(0);
    return conditionCstToAstVisitor.visit(cst!) as Expression;
  };

  it("should convert identifier expression", () => {
    const ast = parseAndVisit("foo") as any;
    expect(ast.type).toBe("identifier");
    expect(ast.name).toBe("foo");
  });

  it("should convert number literal", () => {
    const ast = parseAndVisit("42") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("number");
    expect(ast.value).toBe(42);
  });

  it("should convert float literal", () => {
    const ast = parseAndVisit("3.14") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("number");
    expect(ast.value).toBe(3.14);
  });

  it("should convert string literal", () => {
    const ast = parseAndVisit("'hello'") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("string");
    expect(ast.value).toBe("hello");
  });

  it("should unescape string literals", () => {
    const ast = parseAndVisit("'hello\\'world'") as any;
    expect(ast.value).toBe("hello'world");
  });

  it("should convert unescape string with backslash n", () => {
    const ast = parseAndVisit("'line1\\nline2'") as any;
    expect(ast.value).toBe("line1\nline2");
  });

  it("should convert true literal", () => {
    const ast = parseAndVisit("true") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("boolean");
    expect(ast.value).toBe(true);
  });

  it("should convert false literal", () => {
    const ast = parseAndVisit("false") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("boolean");
    expect(ast.value).toBe(false);
  });

  it("should convert null literal", () => {
    const ast = parseAndVisit("null") as any;
    expect(ast.type).toBe("literal");
    expect(ast.valueType).toBe("null");
    expect(ast.value).toBeNull();
  });

  it("should convert binary expression", () => {
    const ast = parseAndVisit("a + b") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("+");
    expect(ast.left.name).toBe("a");
    expect(ast.right.name).toBe("b");
  });

  it("should convert comparison expression", () => {
    const ast = parseAndVisit("a == b") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("==");
  });

  it("should convert logical OR expression", () => {
    const ast = parseAndVisit("a || b") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("||");
  });

  it("should convert logical AND expression", () => {
    const ast = parseAndVisit("a && b") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("&&");
  });

  it("should convert NOT expression", () => {
    const ast = parseAndVisit("!a") as any;
    expect(ast.type).toBe("not");
    expect(ast.operand.name).toBe("a");
  });

  it("should convert unary minus expression", () => {
    const ast = parseAndVisit("-42") as any;
    expect(ast.type).toBe("unaryMinus");
    expect(ast.operand.value).toBe(42);
  });

  it("should convert double unary minus", () => {
    const ast = parseAndVisit("--5") as any;
    expect(ast.type).toBe("unaryMinus");
    expect(ast.operand.type).toBe("unaryMinus");
  });

  it("should respect operator precedence (multiplication before addition)", () => {
    const ast = parseAndVisit("1 + 2 * 3") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("+");
    expect(ast.left.value).toBe(1);
    expect(ast.right.type).toBe("binary");
    expect(ast.right.operator).toBe("*");
  });

  it("should respect operator precedence (comparison before logical)", () => {
    const ast = parseAndVisit("a > 5 && b < 10") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("&&");
    expect(ast.left.operator).toBe(">");
    expect(ast.right.operator).toBe("<");
  });

  it("should convert ternary expression", () => {
    const ast = parseAndVisit("a ? b : c") as any;
    expect(ast.type).toBe("ternary");
    expect(ast.condition.name).toBe("a");
    expect(ast.consequent.name).toBe("b");
    expect(ast.alternate.name).toBe("c");
  });

  it("should convert nested ternary (right-associative)", () => {
    const ast = parseAndVisit("a ? b ? c : d : e") as any;
    expect(ast.type).toBe("ternary");
    expect(ast.condition.name).toBe("a");
    expect(ast.consequent.type).toBe("ternary");
    expect(ast.consequent.condition.name).toBe("b");
    expect(ast.alternate.name).toBe("e");
  });

  it("should convert contains expression", () => {
    const ast = parseAndVisit("arr contains 5") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("contains");
  });

  it("should convert 'in' expression", () => {
    const ast = parseAndVisit("5 in arr") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("in");
  });

  it("should convert member access expression", () => {
    const ast = parseAndVisit("obj.property") as any;
    expect(ast.type).toBe("memberAccess");
    expect(ast.object.name).toBe("obj");
    expect(ast.property).toBe("property");
  });

  it("should convert chained member access", () => {
    const ast = parseAndVisit("a.b.c") as any;
    expect(ast.type).toBe("memberAccess");
    expect(ast.property).toBe("c");
    expect(ast.object.type).toBe("memberAccess");
    expect(ast.object.property).toBe("b");
    expect(ast.object.object.name).toBe("a");
  });

  it("should convert method call on object", () => {
    const ast = parseAndVisit("arr.someEqual(5)") as any;
    expect(ast.type).toBe("call");
    expect(ast.callee.type).toBe("memberAccess");
    expect(ast.callee.property).toBe("someEqual");
    expect(ast.callee.object.name).toBe("arr");
    expect(ast.arguments).toHaveLength(1);
    expect(ast.arguments[0].value).toBe(5);
    expect(ast.methodKind).toBe("arrayMethod");
  });

  it("should convert function call", () => {
    const ast = parseAndVisit("myFunc(a, b)") as any;
    expect(ast.type).toBe("call");
    expect(ast.callee.type).toBe("identifier");
    expect(ast.callee.name).toBe("myFunc");
    expect(ast.arguments).toHaveLength(2);
  });

  it("should convert function call with no arguments", () => {
    const ast = parseAndVisit("myFunc()") as any;
    expect(ast.type).toBe("call");
    expect(ast.arguments).toHaveLength(0);
  });

  it("should convert string method call", () => {
    const ast = parseAndVisit("str.startsWith('hello')") as any;
    expect(ast.type).toBe("call");
    expect(ast.callee.property).toBe("startsWith");
    expect(ast.methodKind).toBe("stringMethod");
  });

  it("should convert array literal", () => {
    const ast = parseAndVisit("[1, 2, 3]") as any;
    expect(ast.type).toBe("arrayLiteral");
    expect(ast.elements).toHaveLength(3);
    expect(ast.elements[0].value).toBe(1);
    expect(ast.elements[1].value).toBe(2);
    expect(ast.elements[2].value).toBe(3);
  });

  it("should convert empty array literal", () => {
    const ast = parseAndVisit("[]") as any;
    expect(ast.type).toBe("arrayLiteral");
    expect(ast.elements).toHaveLength(0);
  });

  it("should convert mixed array literal", () => {
    const ast = parseAndVisit("['a', true, null]") as any;
    expect(ast.type).toBe("arrayLiteral");
    expect(ast.elements).toHaveLength(3);
    expect(ast.elements[0].value).toBe("a");
    expect(ast.elements[1].value).toBe(true);
    expect(ast.elements[2].value).toBeNull();
  });

  it("should convert subscript access with number", () => {
    const ast = parseAndVisit("arr[0]") as any;
    expect(ast.type).toBe("memberAccess");
    expect(ast.object.name).toBe("arr");
    expect(ast.property).toBe("0");
  });

  it("should convert subscript access with string key", () => {
    const ast = parseAndVisit("obj['key']") as any;
    expect(ast.type).toBe("memberAccess");
    expect(ast.property).toBe("key");
  });

  it("should handle chained method calls", () => {
    const ast = parseAndVisit("arr.map(x).distinct()") as any;
    expect(ast.type).toBe("call");
    expect(ast.callee.property).toBe("distinct");
    expect(ast.callee.object.type).toBe("call");
    expect(ast.callee.object.callee.property).toBe("map");
  });

  it("should include metadata with location", () => {
    const ast = parseAndVisit("42") as any;
    expect(ast.metadata).toBeDefined();
    expect(ast.metadata.location).toBeDefined();
    expect(ast.metadata.location.start).toBeGreaterThanOrEqual(0);
  });

  it("should convert parenthesized expression", () => {
    const ast = parseAndVisit("(a + b) * c") as any;
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("*");
    expect(ast.left.type).toBe("binary");
    expect(ast.left.operator).toBe("+");
    expect(ast.right.name).toBe("c");
  });

  it("should throw for unknown CST node", () => {
    const fakeCst = { name: "unknown", children: {} } as any;
    expect(() => conditionCstToAstVisitor.visit(fakeCst)).toThrow("Unknown CST node type: unknown");
  });
});

describe("Public API - index.ts", () => {
  describe("dslParse", () => {
    it("should parse a simple expression", () => {
      const ast = dslParse("foo > 5");
      expect(ast).toBeDefined();
      expect(ast.type).toBe("binary");
    });

    it("should throw on invalid expression", () => {
      expect(() => dslParse("a +")).toThrow("Parsing failed");
    });

    it("should throw on empty string", () => {
      expect(() => dslParse("")).toThrow();
    });
  });

  describe("dslParseWithErrors", () => {
    it("should return AST for valid expression", () => {
      const result = dslParseWithErrors("foo");
      expect(result.ast).not.toBeNull();
      expect(result.errors).toHaveLength(0);
    });

    it("should return errors for invalid expression", () => {
      const result = dslParseWithErrors("a +");
      expect(result.ast).toBeNull();
      expect(result.errors.length).toBeGreaterThan(0);
    });

    it("should handle lexer errors", () => {
      const result = dslParseWithErrors("@invalid");
      expect(result.ast).toBeNull();
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe("dslValidate", () => {
    it("should return valid for correct expression", () => {
      const result = dslValidate("a == b");
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("should return invalid for incorrect expression", () => {
      const result = dslValidate("a +");
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe("parseToCst", () => {
    it("should parse to CST successfully", () => {
      const result = parseToCst("foo");
      expect(result.cst).not.toBeNull();
      expect(result.errors).toHaveLength(0);
      expect(result.cst!.name).toBe("expression");
    });

    it("should handle lexer errors in parseToCst", () => {
      const result = parseToCst("@invalid");
      expect(result.cst).toBeNull();
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.errors[0]!.severity).toBe("error");
    });

    it("should handle parse errors gracefully", () => {
      const result = parseToCst("a +");
      expect(result.errors.length).toBeGreaterThan(0);
    });

    it("should handle unexpected errors gracefully", () => {
      const result = parseToCst("");
      expect(result.cst).not.toBeNull();
      expect(result.errors.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe("cstToAst", () => {
    it("should convert CST to AST", () => {
      const { cst } = parseToCst("42");
      const ast = cstToAst(cst!);
      expect(ast.type).toBe("literal");
    });
  });

  describe("tokenizeExpression", () => {
    it("should return tokens for valid expression", () => {
      const tokens = tokenizeExpression("a + b");
      expect(tokens).toHaveLength(3);
    });
  });

  describe("Complex expressions", () => {
    it("should handle complex logical expression", () => {
      const ast = dslParse("(a > 5 && b < 10) || c == 0") as any;
      expect(ast.type).toBe("binary");
      expect(ast.operator).toBe("||");
    });

    it("should handle method chaining with filter", () => {
      const ast = dslParse("items.someEqual(user.id)") as any;
      expect(ast.type).toBe("call");
      expect(ast.callee.property).toBe("someEqual");
    });

    it("should handle arithmetic with variables", () => {
      const ast = dslParse("price * quantity + tax") as any;
      expect(ast.type).toBe("binary");
      expect(ast.operator).toBe("+");
    });

    it("should handle expression with string methods", () => {
      const ast = dslParse("name.toLowerCase()") as any;
      expect(ast.type).toBe("call");
      expect(ast.methodKind).toBe("stringMethod");
    });
  });
});