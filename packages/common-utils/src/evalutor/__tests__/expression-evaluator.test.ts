/**
 * ExpressionEvaluator Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { ExpressionEvaluator, expressionEvaluator } from "../expression-evaluator.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { EvaluationContext } from "@wf-agent/types";
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

describe("ExpressionEvaluator", () => {
  let evaluator: ExpressionEvaluator;
  let context: EvaluationContext;

  beforeEach(() => {
    evaluator = new ExpressionEvaluator();
    context = {
      input: {
        status: "active",
        score: 85,
        tags: ["admin", "user"],
        user: {
          name: "Alice",
          age: 30,
        },
      },
      output: {
        result: {
          success: true,
          message: "OK",
        },
        count: 10,
      },
      variables: {
        user: {
          age: 25,
          name: "John",
          role: "admin",
          email: "john@example.com",
        },
        maxAge: 65,
        minAge: 18,
        isActive: true,
        text: "Hello World",
        emptyString: "",
        zero: 0,
        negative: -10,
      },
    };
  });

  describe("evaluate - Basic functionality", () => {
    it("Simple equal expressions should be evaluated.", () => {
      expect(evaluator.evaluate("user.age == 25", context)).toBe(true);
      expect(evaluator.evaluate("user.age == 30", context)).toBe(false);
    });

    it("The 'should evaluate to not equal' expression should be assessed.", () => {
      expect(evaluator.evaluate("user.age != 30", context)).toBe(true);
      expect(evaluator.evaluate("user.age != 25", context)).toBe(false);
    });

    it("The expression that should be evaluated is greater than...", () => {
      expect(evaluator.evaluate("user.age > 20", context)).toBe(true);
      expect(evaluator.evaluate("user.age > 25", context)).toBe(false);
    });

    it("The expression should be evaluated for values that are less than it.", () => {
      expect(evaluator.evaluate("user.age < 30", context)).toBe(true);
      expect(evaluator.evaluate("user.age < 25", context)).toBe(false);
    });

    it("The expression 'greater than or equal to' should be evaluated.", () => {
      expect(evaluator.evaluate("user.age >= 25", context)).toBe(true);
      expect(evaluator.evaluate("user.age >= 26", context)).toBe(false);
    });

    it("The expression 'less than or equal to' should be evaluated.", () => {
      expect(evaluator.evaluate("user.age <= 25", context)).toBe(true);
      expect(evaluator.evaluate("user.age <= 24", context)).toBe(false);
    });

    it("The expressions contained within should be evaluated.", () => {
      expect(evaluator.evaluate("user.name contains 'oh'", context)).toBe(true);
      expect(evaluator.evaluate("user.name contains 'xyz'", context)).toBe(false);
    });

    it("The expression `should be evaluated in` should be translated to 'The expression should be evaluated in'.", () => {
      expect(evaluator.evaluate("user.role in ['admin', 'user']", context)).toBe(true);
      expect(evaluator.evaluate("user.role in ['guest', 'user']", context)).toBe(false);
    });
  });

  describe("Evaluate - Logical operations", () => {
    it("The AND expression should be evaluated.", () => {
      expect(evaluator.evaluate("user.age >= 18 && user.age <= 65", context)).toBe(true);
      expect(evaluator.evaluate("user.age >= 18 && user.age > 65", context)).toBe(false);
    });

    it("The OR expression should be evaluated.", () => {
      expect(evaluator.evaluate('user.age < 18 || user.role == "admin"', context)).toBe(true);
      expect(evaluator.evaluate('user.age < 18 || user.role == "guest"', context)).toBe(false);
    });

    it("The mixed logical expressions should be evaluated.", () => {
      expect(evaluator.evaluate('user.age >= 18 && user.role == "admin"', context)).toBe(true);
      expect(
        evaluator.evaluate('(user.age >= 18 && user.age <= 65) || user.role == "admin"', context),
      ).toBe(true);
    });

    it("Short-circuit evaluation should be supported.", () => {
      expect(evaluator.evaluate("false && user.nonexistent == 123", context)).toBe(false);
      expect(evaluator.evaluate("true || user.nonexistent == 123", context)).toBe(true);
    });
  });

  describe("`evaluate - NOT operation`", () => {
    it("The NOT expression should be evaluated.", () => {
      expect(evaluator.evaluate("!isActive", context)).toBe(false);
      expect(evaluator.evaluate("!(user.age < 18)", context)).toBe(true);
    });

    it("The nested NOT expression should be evaluated.", () => {
      expect(evaluator.evaluate("!!isActive", context)).toBe(true);
      expect(evaluator.evaluate("!(!isActive)", context)).toBe(true);
    });
  });

  describe("evaluate - Arithmetic operation", () => {
    it("The addition expression should be evaluated.", () => {
      // Note: Since the parser parses variables as comparison nodes, a numeric literal is used for testing here.
      expect(evaluator.evaluate("25 + 5", context)).toBe(30);
    });

    it("The subtraction expression should be evaluated.", () => {
      expect(evaluator.evaluate("25 - 5", context)).toBe(20);
    });

    it("The multiplication expression should be evaluated.", () => {
      expect(evaluator.evaluate("25 * 2", context)).toBe(50);
    });

    it("The division expression should be evaluated.", () => {
      expect(evaluator.evaluate("25 / 5", context)).toBe(5);
    });

    it("The modulus expression should be evaluated.", () => {
      expect(evaluator.evaluate("25 % 7", context)).toBe(4);
    });

    it("Zero division errors should be handled.", () => {
      expect(evaluator.evaluate("25 / 0", context)).toBeNaN();
    });

    it("Arithmetic operations with mismatched types should be handled accordingly.", () => {
      expect(evaluator.evaluate('"hello" + 5', context)).toBeNaN();
    });

    it("Complex arithmetic expressions should be evaluated.", () => {
      // Note: Since the parser processes from left to right, the priorities of * / % and + - are the same.
      // So '25 * 2 + 10' is parsed as '25 * (2 + 10)' = 300
      expect(evaluator.evaluate("25 * 2 + 10", context)).toBe(300);
      // Use parentheses to clarify the priority.
      expect(evaluator.evaluate("(25 + 5) * 2", context)).toBe(60);
    });
  });

  describe("`evaluate` - A string method", () => {
    it("The `startsWith` method should be evaluated.", () => {
      expect(evaluator.evaluate('text.startsWith("Hello")', context)).toBe(true);
      expect(evaluator.evaluate('text.startsWith("World")', context)).toBe(false);
    });

    it("The endsWith method should be evaluated.", () => {
      expect(evaluator.evaluate('text.endsWith("World")', context)).toBe(true);
      expect(evaluator.evaluate('text.endsWith("Hello")', context)).toBe(false);
    });

    it("The `length` method should be evaluated.", () => {
      expect(evaluator.evaluate("text.length", context)).toBe(11);
    });

    it("The `toLowerCase` method should be evaluated.", () => {
      expect(evaluator.evaluate("text.toLowerCase()", context)).toBe("hello world");
    });

    it("The `toUpperCase` method should be evaluated.", () => {
      expect(evaluator.evaluate("text.toUpperCase()", context)).toBe("HELLO WORLD");
    });

    it("The `trim` method should be evaluated.", () => {
      expect(evaluator.evaluate("text.trim()", context)).toBe("Hello World");
    });

    it("Methods that should handle string values that are not actually strings", () => {
      expect(evaluator.evaluate("user.age.length", context)).toBe(false);
    });
  });

  describe("`evaluate` - Ternary Operator", () => {
    it("The ternary expression whose condition should be evaluated to true should be considered.", () => {
      expect(evaluator.evaluate('user.age >= 18 ? "adult" : "minor"', context)).toBe("adult");
    });

    it("The ternary expression whose condition should be evaluated as false.", () => {
      expect(evaluator.evaluate('user.age < 18 ? "minor" : "adult"', context)).toBe("adult");
    });

    it("The nested ternary expressions should be evaluated.", () => {
      expect(
        evaluator.evaluate('user.age < 18 ? "minor" : user.age < 65 ? "adult" : "senior"', context),
      ).toBe("adult");
    });

    it("The ternary expression that evaluates complex conditions should be assessed.", () => {
      expect(evaluator.evaluate('user.role == "admin" ? "admin" : "user"', context)).toBe("admin");
    });
  });

  describe("evaluate - Data source access", () => {
    it("Expressions should be evaluated from the input data source", () => {
      expect(evaluator.evaluate('input.status == "active"', context)).toBe(true);
      expect(evaluator.evaluate("input.score > 80", context)).toBe(true);
    });

    it("Expressions should be evaluated from the output data source", () => {
      expect(evaluator.evaluate("output.result.success == true", context)).toBe(true);
      expect(evaluator.evaluate("output.count == 10", context)).toBe(true);
    });

    it("Expressions should be evaluated from the variables data source (explicit prefix)", () => {
      expect(evaluator.evaluate("variables.user.age == 25", context)).toBe(true);
      expect(evaluator.evaluate("variables.maxAge == 65", context)).toBe(true);
    });

    it("Expressions (simple variable names) should be evaluated from the variables data source", () => {
      expect(evaluator.evaluate("maxAge == 65", context)).toBe(true);
      expect(evaluator.evaluate("minAge == 18", context)).toBe(true);
    });

    it("Nested paths should be evaluated from the variables data source", () => {
      expect(evaluator.evaluate('user.email == "john@example.com"', context)).toBe(true);
    });
  });

  describe("evaluate - literal", () => {
    it("Boolean literals should be evaluated", () => {
      expect(evaluator.evaluate("true", context)).toBe(true);
      expect(evaluator.evaluate("false", context)).toBe(false);
    });

    it("Digital literal quantities should be assessed", () => {
      expect(evaluator.evaluate("42", context)).toBe(42);
      expect(evaluator.evaluate("-10", context)).toBe(-10);
      expect(evaluator.evaluate("3.14", context)).toBe(3.14);
    });

    it("String literals should be evaluated", () => {
      expect(evaluator.evaluate('"hello"', context)).toBe("hello");
      expect(evaluator.evaluate("'world'", context)).toBe("world");
    });

    it("The null literal should be evaluated", () => {
      expect(evaluator.evaluate("null", context)).toBe(null);
    });
  });

  describe("evaluate - boundary conditions", () => {
    it("Should handle empty string comparisons", () => {
      expect(evaluator.evaluate('emptyString == ""', context)).toBe(true);
    });

    it("Zero-value comparisons should be handled", () => {
      expect(evaluator.evaluate("zero == 0", context)).toBe(true);
    });

    it("Negative comparisons should be handled", () => {
      expect(evaluator.evaluate("negative > -20", context)).toBe(true);
    });

    it("Should handle floating point comparisons", () => {
      expect(evaluator.evaluate("user.age > 24.5", context)).toBe(true);
    });

    it("Non-existent variables should be handled", () => {
      expect(evaluator.evaluate("nonexistent == 123", context)).toBe(false);
    });

    it("Comparisons of mismatched types should be handled", () => {
      expect(evaluator.evaluate("user.name > 100", context)).toBe(false);
    });

    it("Non-array values of the in operator should be handled.", () => {
      expect(evaluator.evaluate('user.age in "not an array"', context)).toBe(false);
    });
  });

  describe("evaluateAST - literal nodes", () => {
    it("Boolean literal nodes should be evaluated", () => {
      const node: BooleanLiteralNode = { type: "boolean", value: true };
      expect(evaluator.evaluateAST(node, context)).toBe(true);

      const node2: BooleanLiteralNode = { type: "boolean", value: false };
      expect(evaluator.evaluateAST(node2, context)).toBe(false);
    });

    it("Digital Literal Quantity nodes should be evaluated", () => {
      const node: NumberLiteralNode = { type: "number", value: 42 };
      expect(evaluator.evaluateAST(node, context)).toBe(42);

      const node2: NumberLiteralNode = { type: "number", value: -10.5 };
      expect(evaluator.evaluateAST(node2, context)).toBe(-10.5);
    });

    it("String literal nodes should be evaluated", () => {
      const node: StringLiteralNode = { type: "string", value: "hello" };
      expect(evaluator.evaluateAST(node, context)).toBe("hello");
    });

    it("The null literal node should be evaluated", () => {
      const node: NullLiteralNode = { type: "null", value: null };
      expect(evaluator.evaluateAST(node, context)).toBe(null);
    });
  });

  describe("evaluateAST - Compare Nodes", () => {
    it("Should evaluate equals compare nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "==",
        value: 25,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("Should evaluate is not the same as compare nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "!=",
        value: 30,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("Should evaluate more than compare nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: ">",
        value: 20,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("Should evaluate less than comparison nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "<",
        value: 30,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("should be evaluated greater than or equal to the comparison node", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: ">=",
        value: 25,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("should be evaluated less than or equal to the comparison node", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "<=",
        value: 25,
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("It should be evaluated to include comparison nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.name",
        operator: "contains",
        value: "oh",
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("Should evaluate in comparison nodes", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.role",
        operator: "in",
        value: ["admin", "user"],
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("Comparison of variable references should be handled", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "==",
        value: { __isVariableRef: true, path: "maxAge" },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(false);
    });

    it("Unknown operator error should be thrown", () => {
      const node: ComparisonNode = {
        type: "comparison",
        variablePath: "user.age",
        operator: "unknown" as any,
        value: 25,
      };
      expect(() => evaluator.evaluateAST(node, context)).toThrow(RuntimeValidationError);
    });
  });

  describe("evaluateAST - Logical Node", () => {
    it("AND logical nodes should be evaluated", () => {
      const node: LogicalNode = {
        type: "logical",
        operator: "&&",
        left: { type: "comparison", variablePath: "user.age", operator: ">=", value: 18 },
        right: { type: "comparison", variablePath: "user.age", operator: "<=", value: 65 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("The OR logical node should be evaluated", () => {
      const node: LogicalNode = {
        type: "logical",
        operator: "||",
        left: { type: "comparison", variablePath: "user.age", operator: "<", value: 18 },
        right: { type: "comparison", variablePath: "user.role", operator: "==", value: "admin" },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });
  });

  describe("evaluateAST - NOT node", () => {
    it("The NOT node should be evaluated", () => {
      const node: NotNode = {
        type: "not",
        operand: { type: "boolean", value: true },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(false);
    });

    it("Nested NOT nodes should be evaluated", () => {
      const node: NotNode = {
        type: "not",
        operand: {
          type: "not",
          operand: { type: "boolean", value: true },
        },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });
  });

  describe("evaluateAST - Arithmetic Nodes", () => {
    it("Addition nodes should be evaluated", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "+",
        left: { type: "number", value: 10 },
        right: { type: "number", value: 20 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(30);
    });

    it("Subtraction nodes should be evaluated", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "-",
        left: { type: "number", value: 20 },
        right: { type: "number", value: 10 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(10);
    });

    it("Multiplication nodes should be evaluated", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "*",
        left: { type: "number", value: 5 },
        right: { type: "number", value: 4 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(20);
    });

    it("The division node should be evaluated", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "/",
        left: { type: "number", value: 20 },
        right: { type: "number", value: 4 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(5);
    });

    it("The mode-taking node should be evaluated", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "%",
        left: { type: "number", value: 10 },
        right: { type: "number", value: 3 },
      };
      expect(evaluator.evaluateAST(node, context)).toBe(1);
    });

    it("Should handle de-zeroing", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "/",
        left: { type: "number", value: 10 },
        right: { type: "number", value: 0 },
      };
      expect(evaluator.evaluateAST(node, context)).toBeNaN();
    });

    it("Type mismatch should be handled.", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "+",
        left: { type: "string", value: "hello" },
        right: { type: "number", value: 10 },
      };
      expect(evaluator.evaluateAST(node, context)).toBeNaN();
    });

    it("An error should be thrown for the unknown operator.", () => {
      const node: ArithmeticNode = {
        type: "arithmetic",
        operator: "^" as any,
        left: { type: "number", value: 2 },
        right: { type: "number", value: 3 },
      };
      expect(() => evaluator.evaluateAST(node, context)).toThrow(RuntimeValidationError);
    });
  });

  describe("evaluateAST - String method node", () => {
    it("The `startsWith` method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "startsWith",
        argument: "Hello",
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("The endsWith method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "endsWith",
        argument: "World",
      };
      expect(evaluator.evaluateAST(node, context)).toBe(true);
    });

    it("The `length` method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "length",
      };
      expect(evaluator.evaluateAST(node, context)).toBe(11);
    });

    it("The `toLowerCase` method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "toLowerCase",
      };
      expect(evaluator.evaluateAST(node, context)).toBe("hello world");
    });

    it("The `toUpperCase` method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "toUpperCase",
      };
      expect(evaluator.evaluateAST(node, context)).toBe("HELLO WORLD");
    });

    it("The `trim` method node should be evaluated.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "trim",
      };
      expect(evaluator.evaluateAST(node, context)).toBe("Hello World");
    });

    it("Non-string values should be handled accordingly.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "user.age",
        method: "length",
      };
      expect(evaluator.evaluateAST(node, context)).toBe(false);
    });

    it("An error for an unknown method should be thrown.", () => {
      const node: StringMethodNode = {
        type: "stringMethod",
        variablePath: "text",
        method: "unknown" as any,
      };
      expect(() => evaluator.evaluateAST(node, context)).toThrow(RuntimeValidationError);
    });
  });

  describe("evaluateAST - Ternary Operator Node", () => {
    it("The ternary node whose condition should be evaluated as true should be considered.", () => {
      const node: TernaryNode = {
        type: "ternary",
        condition: { type: "comparison", variablePath: "user.age", operator: ">=", value: 18 },
        consequent: { type: "string", value: "adult" },
        alternate: { type: "string", value: "minor" },
      };
      expect(evaluator.evaluateAST(node, context)).toBe("adult");
    });

    it("The ternary node whose condition should be evaluated as false should be considered.", () => {
      const node: TernaryNode = {
        type: "ternary",
        condition: { type: "comparison", variablePath: "user.age", operator: "<", value: 18 },
        consequent: { type: "string", value: "minor" },
        alternate: { type: "string", value: "adult" },
      };
      expect(evaluator.evaluateAST(node, context)).toBe("adult");
    });

    it("The nested ternary nodes should be evaluated.", () => {
      const node: TernaryNode = {
        type: "ternary",
        condition: { type: "comparison", variablePath: "user.age", operator: "<", value: 18 },
        consequent: { type: "string", value: "minor" },
        alternate: {
          type: "ternary",
          condition: { type: "comparison", variablePath: "user.age", operator: "<", value: 65 },
          consequent: { type: "string", value: "adult" },
          alternate: { type: "string", value: "senior" },
        },
      };
      expect(evaluator.evaluateAST(node, context)).toBe("adult");
    });
  });

  describe("evaluateAST - Error Handling", () => {
    it("An error should be thrown for an unknown node type.", () => {
      const node = { type: "unknown" } as any;
      expect(() => evaluator.evaluateAST(node, context)).toThrow(RuntimeValidationError);
    });
  });

  describe("getVariableValue - Data source access rules", () => {
    it("Values should be obtained from the input data source.", () => {
      expect(evaluator["getVariableValue"]("input.status", context)).toBe("active");
      expect(evaluator["getVariableValue"]("input.user.name", context)).toBe("Alice");
    });

    it("The values should be obtained from the output data source.", () => {
      expect(evaluator["getVariableValue"]("output.result.success", context)).toBe(true);
      expect(evaluator["getVariableValue"]("output.count", context)).toBe(10);
    });

    it("Values should be obtained from the variables data source (with an explicit prefix).", () => {
      expect(evaluator["getVariableValue"]("variables.user.age", context)).toBe(25);
      expect(evaluator["getVariableValue"]("variables.maxAge", context)).toBe(65);
    });

    it("Values should be obtained from the variable data source (using simple variable names).", () => {
      expect(evaluator["getVariableValue"]("maxAge", context)).toBe(65);
      expect(evaluator["getVariableValue"]("minAge", context)).toBe(18);
    });

    it("The nested path values should be obtained from the variables data source.", () => {
      expect(evaluator["getVariableValue"]("user.email", context)).toBe("john@example.com");
      expect(evaluator["getVariableValue"]("user.role", context)).toBe("admin");
    });

    it("It should return `undefined` for non-existent variables.", () => {
      expect(evaluator["getVariableValue"]("nonexistent", context)).toBeUndefined();
      expect(evaluator["getVariableValue"]("user.nonexistent", context)).toBeUndefined();
    });
  });
});

describe("ExpressionEvaluator Singleton", () => {
  it("The singleton instance should be exported.", () => {
    expect(expressionEvaluator).toBeInstanceOf(ExpressionEvaluator);
  });

  it("The singleton should be working properly.", () => {
    const context: EvaluationContext = {
      input: {},
      output: {},
      variables: { age: 25 },
    };
    expect(expressionEvaluator.evaluate("age == 25", context)).toBe(true);
  });

  it("Singletons should handle complex expressions.", () => {
    const context: EvaluationContext = {
      input: {},
      output: {},
      variables: {
        age: 25,
        role: "admin",
        text: "Hello",
      },
    };
    expect(expressionEvaluator.evaluate('age >= 18 && role == "admin"', context)).toBe(true);
    expect(expressionEvaluator.evaluate('text.startsWith("He")', context)).toBe(true);
  });
});
