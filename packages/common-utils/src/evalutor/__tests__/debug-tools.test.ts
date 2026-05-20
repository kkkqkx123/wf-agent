/**
 * Debugging Tools Tests
 * Tests for expression visualization and debugging (Phase 2)
 */

import { describe, it, expect } from "vitest";
import { parseAST } from "../expression-parser.js";
import { visualizeAST, traceEvaluation, formatTrace } from "../debug-tools.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("Debugging Tools", () => {
  const context: EvaluationContext = {
    variables: {
      user: {
        name: "Alice",
        age: 30,
        active: true,
      },
    },
    input: {},
    output: {},
  };

  describe("visualizeAST", () => {
    it("should visualize simple comparison", () => {
      const ast = parseAST("user.age > 18");
      const tree = visualizeAST(ast);
      
      expect(tree).toContain("Comparison");
      expect(tree).toContain("user.age");
      expect(tree).toContain(">");
    });

    it("should visualize logical expression", () => {
      const ast = parseAST("user.age > 18 && user.active == true");
      const tree = visualizeAST(ast);
      
      expect(tree).toContain("Logical");
      expect(tree).toContain("&&");
      expect(tree).toContain("Comparison");
    });

    it("should visualize nested structure with indentation", () => {
      const ast = parseAST("user.age > 18 && user.active == true");
      const tree = visualizeAST(ast);
      
      // Should have multiple lines
      const lines = tree.split("\n");
      expect(lines.length).toBeGreaterThan(2);
      
      // Should have indentation for child nodes
      expect(tree).toMatch(/  +/); // At least one indented line
    });

    it("should visualize arithmetic expression", () => {
      const ast = parseAST("user.age * 2");
      const tree = visualizeAST(ast);
      
      expect(tree).toContain("Arithmetic");
      expect(tree).toContain("*");
    });

    it("should visualize boolean literal", () => {
      const ast = parseAST("true");
      const tree = visualizeAST(ast);
      
      expect(tree).toContain("Boolean");
      expect(tree).toContain("true");
    });

    it("should visualize string literal", () => {
      const ast = parseAST("'hello'");
      const tree = visualizeAST(ast);
      
      expect(tree).toContain("String");
      expect(tree).toContain("hello");
    });
  });

  describe("traceEvaluation", () => {
    it("should trace simple expression", () => {
      const trace = traceEvaluation("user.age > 18", context);
      
      expect(trace.expression).toBe("user.age > 18");
      expect(trace.result).toBe(true);
      expect(trace.totalTime).toBeGreaterThanOrEqual(0);
      expect(trace.root).toBeDefined();
    });

    it("should trace logical expression", () => {
      const trace = traceEvaluation("user.age > 18 && user.active == true", context);
      
      expect(trace.result).toBe(true);
      expect(trace.root.type).toBe("logical");
      expect(trace.root.children).toBeDefined();
      expect(trace.root.children?.length).toBe(2);
    });

    it("should include execution time for nodes", () => {
      const trace = traceEvaluation("user.age > 18", context);
      
      expect(trace.root.executionTime).toBeDefined();
      expect(trace.root.executionTime!).toBeGreaterThanOrEqual(0);
    });

    it("should handle false result", () => {
      const trace = traceEvaluation("user.age < 18", context);
      
      expect(trace.result).toBe(false);
    });

    it("should trace arithmetic expression", () => {
      const trace = traceEvaluation("user.age * 2", context);
      
      expect(trace.result).toBe(60);
      expect(trace.root.type).toBe("arithmetic");
    });
  });

  describe("formatTrace", () => {
    it("should format trace as readable text", () => {
      const trace = traceEvaluation("user.age > 18", context);
      const formatted = formatTrace(trace);
      
      expect(formatted).toContain("Expression:");
      expect(formatted).toContain("Result:");
      expect(formatted).toContain("Total Time:");
      expect(formatted).toContain("Trace:");
    });

    it("should include node types in formatted output", () => {
      const trace = traceEvaluation("user.age > 18", context);
      const formatted = formatTrace(trace);
      
      expect(formatted).toContain("[comparison]");
    });

    it("should include execution times", () => {
      const trace = traceEvaluation("user.age > 18", context);
      const formatted = formatTrace(trace);
      
      expect(formatted).toMatch(/\d+\.\d+ms/);
    });

    it("should format nested expressions", () => {
      const trace = traceEvaluation("user.age > 18 && user.active == true", context);
      const formatted = formatTrace(trace);
      
      // Should have multiple levels of indentation
      const lines = formatted.split("\n");
      const indentedLines = lines.filter(line => line.trim().startsWith("["));
      expect(indentedLines.length).toBeGreaterThan(1);
    });

    it("should handle custom indentation", () => {
      const trace = traceEvaluation("user.age > 18", context);
      const formatted = formatTrace(trace, 2);
      
      // Should start with 4 spaces (2 levels * 2 spaces)
      expect(formatted.startsWith("    ")).toBe(true);
    });
  });

  describe("Integration", () => {
    it("should work end-to-end: parse -> visualize -> trace", () => {
      const expression = "user.age > 18 && user.active == true";
      
      // Parse
      const ast = parseAST(expression);
      expect(ast.type).toBe("logical");
      
      // Visualize
      const tree = visualizeAST(ast);
      expect(tree).toContain("Logical");
      
      // Trace
      const trace = traceEvaluation(expression, context);
      expect(trace.result).toBe(true);
      
      // Format
      const formatted = formatTrace(trace);
      expect(formatted).toContain(expression);
    });

    it("should handle complex expressions", () => {
      const expression = "user.age > 18 && user.active == true && user.name == 'Alice'";
      
      const trace = traceEvaluation(expression, context);
      expect(trace.result).toBe(true);
      
      const formatted = formatTrace(trace);
      expect(formatted.length).toBeGreaterThan(100); // Should be detailed
    });

    it("should provide debugging information for errors", () => {
      // Even if expression evaluates to false, we should get full trace
      const trace = traceEvaluation("user.age < 18", context);
      
      expect(trace.result).toBe(false);
      expect(trace.root).toBeDefined();
      expect(trace.totalTime).toBeGreaterThanOrEqual(0);
    });
  });
});
