/**
 * Condition Evaluator Unit Testing
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { ConditionEvaluator, conditionEvaluator } from "../condition-evaluator.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { Condition, EvaluationContext } from "@wf-agent/types";

describe("ConditionEvaluator", () => {
  let evaluator: ConditionEvaluator;
  let context: EvaluationContext;

  beforeEach(() => {
    evaluator = new ConditionEvaluator();
    context = {
      input: {
        status: "active",
        score: 85,
        tags: ["admin", "user"],
      },
      output: {
        result: {
          success: true,
          message: "OK",
        },
      },
      variables: {
        user: {
          age: 25,
          name: "John",
          role: "admin",
        },
        maxAge: 65,
        minAge: 18,
      },
    };
  });

  describe("evaluate - Basic Functions", () => {
    it("Simple equals conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age == 25",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Simple inequality conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age != 30",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Should be evaluated greater than condition", () => {
      const condition: Condition = {
        expression: "user.age > 20",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Less than condition should be assessed", () => {
      const condition: Condition = {
        expression: "user.age < 30",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Should assess greater than or equal to conditions", () => {
      const condition: Condition = {
        expression: "user.age >= 25",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Should assess less than or equal to conditions", () => {
      const condition: Condition = {
        expression: "user.age <= 25",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Inclusion conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.name contains 'oh'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Should evaluate the condition in the array", () => {
      const condition: Condition = {
        expression: "user.role in ['admin', 'user']",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });
  });

  describe("evaluate - composite conditions", () => {
    it("AND conditions should be assessed", () => {
      const condition: Condition = {
        expression: "user.age >= 18 && user.age <= 65",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("OR conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age < 18 || user.role == 'admin'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Mixed logic conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age >= 18 && user.role == 'admin'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Bracketed conditions should be assessed", () => {
      const condition: Condition = {
        expression: "(user.age >= 18 && user.age <= 65) || user.role == 'admin'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });
  });

  describe("evaluate - data source access", () => {
    it("Conditions should be evaluated from the input data source", () => {
      const condition: Condition = {
        expression: "input.status == 'active'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Conditions should be evaluated from the output data source", () => {
      const condition: Condition = {
        expression: "output.result.success == true",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Conditions should be evaluated from the variables data source (explicit prefix)", () => {
      const condition: Condition = {
        expression: "variables.user.age == 25",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Conditions should be evaluated from the variables data source (simple variable names)", () => {
      const condition: Condition = {
        expression: "maxAge == 65",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });
  });

  describe("evaluate - Error Handling", () => {
    it("should throw a missing expression field error", () => {
      const condition: Condition = {
        expression: "",
      };
      expect(() => {
        evaluator.evaluate(condition, context);
      }).toThrow(RuntimeValidationError);
    });

    it("An invalid expression error should be thrown", () => {
      const condition: Condition = {
        expression: "invalid expression",
      };
      expect(() => {
        evaluator.evaluate(condition, context);
      }).toThrow(RuntimeValidationError);
    });

    it("Unknown operator error should be thrown", () => {
      const condition: Condition = {
        expression: "user.age unknown 25",
      };
      expect(() => {
        evaluator.evaluate(condition, context);
      }).toThrow(RuntimeValidationError);
    });

    it("Runtime evaluation failures should be handled", () => {
      const condition: Condition = {
        expression: "nonexistent.value == 123",
      };
      // Non-existent variables should return false instead of throwing an error
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });
  });

  describe("evaluate - boundary conditions", () => {
    it("Boolean true should be evaluated", () => {
      const condition: Condition = {
        expression: "true",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Boolean value should be evaluated false", () => {
      const condition: Condition = {
        expression: "false",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });

    it("The null value should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age == null",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });

    it("Empty strings should be handled", () => {
      const condition: Condition = {
        expression: "user.name == ''",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });

    it("Zero values should be handled", () => {
      const condition: Condition = {
        expression: "user.age == 0",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });

    it("Negative comparisons should be handled", () => {
      const condition: Condition = {
        expression: "user.age > -100",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Should handle floating point comparisons", () => {
      const condition: Condition = {
        expression: "user.age > 24.5",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });
  });

  describe("evaluate - Complex Scenarios", () => {
    it("Multiple nested conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age >= 18 && user.age <= 65 && user.role == 'admin'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Conditions with array indexes should be evaluated", () => {
      const condition: Condition = {
        expression: "input.tags[0] == 'admin'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Conditions with nested objects should be evaluated", () => {
      const condition: Condition = {
        expression: "output.result.message == 'OK'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(true);
    });

    it("Variable referencing conditions should be evaluated", () => {
      const condition: Condition = {
        expression: "user.age == maxAge",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });
  });

  describe("evaluate - type-safe", () => {
    it("Comparisons of mismatched types should be handled", () => {
      const condition: Condition = {
        expression: "user.name > 100",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });

    it("Non-array values of the in operator should be handled.", () => {
      const condition: Condition = {
        expression: "user.age in 'not an array'",
      };
      expect(evaluator.evaluate(condition, context)).toBe(false);
    });
  });
});

describe("conditionEvaluator singleton", () => {
  it("Single instance instances should be exported", () => {
    expect(conditionEvaluator).toBeInstanceOf(ConditionEvaluator);
  });

  it("The singleton should work properly", () => {
    const context: EvaluationContext = {
      input: {},
      output: {},
      variables: { age: 25 },
    };
    const condition: Condition = {
      expression: "age == 25",
    };
    expect(conditionEvaluator.evaluate(condition, context)).toBe(true);
  });

  it("The singleton should handle error cases", () => {
    const context: EvaluationContext = {
      input: {},
      output: {},
      variables: { age: 25 },
    };
    const condition: Condition = {
      expression: "",
    };
    expect(() => {
      conditionEvaluator.evaluate(condition, context);
    }).toThrow(RuntimeValidationError);
  });
});
