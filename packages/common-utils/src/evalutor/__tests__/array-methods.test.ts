/**
 * Array Methods Unit Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ExpressionEvaluator } from "../expression-evaluator.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("Array Helper Functions", () => {
  let evaluator: ExpressionEvaluator;
  let context: EvaluationContext;
  
  beforeEach(() => {
    evaluator = new ExpressionEvaluator();
    context = {
      input: {
        messages: [
          { role: 'user', content: 'Hello' },
          { role: 'assistant', content: 'Hi there!' },
          { role: 'user', content: 'How are you?' },
          { role: 'assistant', content: 'I am fine, thank you!' },
          { role: 'user', content: 'I need help with this' }
        ],
        forkResults: [
          { forkPathId: 'path1', status: 'COMPLETED', output: { data: 1 } },
          { forkPathId: 'path2', status: 'COMPLETED', output: { data: 2 } },
          { forkPathId: 'path3', status: 'FAILED', error: 'Timeout' }
        ]
      },
      output: {},
      variables: {}
    };
  });
  
  describe("Enhanced 'in' operator", () => {
    it("should check object property against array", () => {
      const testContext = {
        ...context,
        input: {
          lastMessage: { role: 'user', content: 'Hello' }
        }
      };
      
      const result = evaluator.evaluate(
        "input.lastMessage in ['user', 'assistant']",
        testContext
      );
      
      expect(result).toBe(true);
    });
    
    it("should return false when no property matches", () => {
      const testContext = {
        ...context,
        input: {
          lastMessage: { role: 'system', content: 'System msg' }
        }
      };
      
      const result = evaluator.evaluate(
        "input.lastMessage in ['user', 'assistant']",
        testContext
      );
      
      expect(result).toBe(false);
    });
    
    it("should work with primitive values (backward compatible)", () => {
      const testContext = {
        ...context,
        variables: {
          role: 'admin'
        }
      };
      
      const result = evaluator.evaluate(
        "role in ['admin', 'user']",
        testContext
      );
      
      expect(result).toBe(true);
    });
  });
  
  describe("someEqual", () => {
    it("should return true when any item matches", () => {
      const result = evaluator.evaluate(
        "input.messages.someEqual('role', 'user')",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should return false when no item matches", () => {
      const result = evaluator.evaluate(
        "input.messages.someEqual('role', 'system')",
        context
      );
      expect(result).toBe(false);
    });
  });
  
  describe("someContains", () => {
    it("should find substring in property", () => {
      const result = evaluator.evaluate(
        "input.messages.someContains('content', 'help')",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should return false when no match found", () => {
      const result = evaluator.evaluate(
        "input.messages.someContains('content', 'xyz123')",
        context
      );
      expect(result).toBe(false);
    });
  });
  
  describe("everyEqual", () => {
    it("should return true when all items match", () => {
      const testContext = {
        ...context,
        input: {
          messages: [
            { role: 'user', content: 'A' },
            { role: 'user', content: 'B' }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.everyEqual('role', 'user')",
        testContext
      );
      expect(result).toBe(true);
    });
    
    it("should return false when not all items match", () => {
      const result = evaluator.evaluate(
        "input.messages.everyEqual('role', 'user')",
        context
      );
      expect(result).toBe(false);  // Has assistant messages too
    });
  });
  
  describe("everyHas", () => {
    it("should return true when all items have the property", () => {
      const testContext = {
        ...context,
        input: {
          messages: [
            { role: 'user', content: 'A' },
            { role: 'assistant', content: 'B' }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.everyHas('content')",
        testContext
      );
      expect(result).toBe(true);
    });
    
    it("should return false when some items missing the property", () => {
      const testContext = {
        ...context,
        input: {
          messages: [
            { role: 'user', content: 'A' },
            { role: 'assistant' }  // Missing content
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.everyHas('content')",
        testContext
      );
      expect(result).toBe(false);
    });
  });
  
  describe("countWhere", () => {
    it("should count matching items", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user')",
        context
      );
      expect(result).toBe(3);
    });
    
    it("should work in comparisons", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'assistant') > 1",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should work with >= operator", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') >= 3",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should work with == operator", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'assistant') == 2",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should work with < operator", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'system') < 1",
        context
      );
      expect(result).toBe(true);
    });
  });
  
  describe("countWhereContains", () => {
    it("should count items containing substring", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhereContains('content', '!')",
        context
      );
      expect(result).toBe(2);  // 'Hi there!' and 'I am fine, thank you!'
    });
  });
  
  describe("findEqual", () => {
    it("should return first matching item", () => {
      const result = evaluator.evaluate(
        "input.messages.findEqual('role', 'user')",
        context
      );
      expect(result).toEqual({ role: 'user', content: 'Hello' });
    });
    
    it("should return null when no match", () => {
      const result = evaluator.evaluate(
        "input.messages.findEqual('role', 'system')",
        context
      );
      expect(result).toBeNull();
    });
  });
  
  describe("findContains", () => {
    it("should return first item containing substring", () => {
      const result = evaluator.evaluate(
        "input.messages.findContains('content', 'help')",
        context
      );
      expect(result).toEqual({ role: 'user', content: 'I need help with this' });
    });
    
    it("should return null when no match", () => {
      const result = evaluator.evaluate(
        "input.messages.findContains('content', 'xyz123')",
        context
      );
      expect(result).toBeNull();
    });
  });
  
  describe("has", () => {
    it("should return true when any item matches", () => {
      const result = evaluator.evaluate(
        "input.messages.has('role', 'assistant')",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should return false when no match", () => {
      const result = evaluator.evaluate(
        "input.messages.has('role', 'system')",
        context
      );
      expect(result).toBe(false);
    });
  });
  
  describe("hasContains", () => {
    it("should return true when any item contains substring", () => {
      const result = evaluator.evaluate(
        "input.messages.hasContains('content', 'fine')",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should return false when no match", () => {
      const result = evaluator.evaluate(
        "input.messages.hasContains('content', 'xyz123')",
        context
      );
      expect(result).toBe(false);
    });
  });
  
  describe("Edge Cases", () => {
    it("should handle empty arrays", () => {
      const emptyContext = {
        ...context,
        input: { messages: [] }
      };
      
      expect(evaluator.evaluate("input.messages.someEqual('role', 'user')", emptyContext)).toBe(false);
      expect(evaluator.evaluate("input.messages.everyEqual('role', 'user')", emptyContext)).toBe(true);
      expect(evaluator.evaluate("input.messages.countWhere('role', 'user')", emptyContext)).toBe(0);
      expect(evaluator.evaluate("input.messages.findEqual('role', 'user')", emptyContext)).toBeNull();
    });
    
    it("should handle non-array values gracefully", () => {
      const badContext = {
        ...context,
        input: { messages: "not an array" }
      };
      
      const result = evaluator.evaluate(
        "input.messages.someEqual('role', 'user')",
        badContext
      );
      expect(result).toBe(false);
    });
    
    it("should handle missing properties", () => {
      const incompleteContext = {
        ...context,
        input: {
          messages: [
            { role: 'user' },  // Missing content
            { role: 'assistant', content: 'Hi' }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.someContains('content', 'Hi')",
        incompleteContext
      );
      expect(result).toBe(true);
    });
    
    it("should access nested properties", () => {
      const nestedContext = {
        ...context,
        input: {
          messages: [
            { role: 'user', metadata: { tags: ['important', 'urgent'] } },
            { role: 'assistant', metadata: { tags: ['response'] } },
            { role: 'user', metadata: { tags: ['question', 'help'] } }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.someEqual('metadata.tags.0', 'important')",
        nestedContext
      );
      expect(result).toBe(true);
    });
    
    it("should access deeply nested properties", () => {
      const deepNestedContext = {
        ...context,
        input: {
          items: [
            { data: { info: { value: 10 } } },
            { data: { info: { value: 20 } } },
            { data: { info: { value: 30 } } }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.items.countWhere('data.info.value', 20)",
        deepNestedContext
      );
      expect(result).toBe(1);
    });
  });
  
  describe("Real-world Scenarios", () => {
    it("should detect error patterns in messages", () => {
      const errorContext = {
        ...context,
        input: {
          messages: [
            { role: 'user', content: 'Request data' },
            { role: 'assistant', content: 'ERROR: Invalid input' },
            { role: 'user', content: 'Try again' }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.messages.someContains('content', 'ERROR')",
        errorContext
      );
      expect(result).toBe(true);
    });
    
    it("should verify all fork branches completed", () => {
      const successContext = {
        ...context,
        input: {
          forkResults: [
            { forkPathId: 'p1', status: 'COMPLETED' },
            { forkPathId: 'p2', status: 'COMPLETED' }
          ]
        }
      };
      
      const result = evaluator.evaluate(
        "input.forkResults.everyEqual('status', 'COMPLETED')",
        successContext
      );
      expect(result).toBe(true);
    });
    
    it("should count long conversations", () => {
      const longContext = {
        ...context,
        input: {
          messages: Array.from({ length: 50 }, (_, i) => ({
            role: i % 2 === 0 ? 'user' : 'assistant',
            content: `Message ${i}`
          }))
        }
      };
      
      // Note: Direct comparison with array methods needs enhanced parser support
      // For now, we just test that the method returns the correct count
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user')",
        longContext
      );
      expect(result).toBe(25);
    });
    
    it("should combine with logical operators", () => {
      // Test combining array methods with && operator
      const result = evaluator.evaluate(
        "input.messages.someEqual('role', 'user') && input.messages.countWhere('role', 'assistant') >= 2",
        context
      );
      expect(result).toBe(true);
      
      // Test combining array methods with || operator
      const result2 = evaluator.evaluate(
        "input.messages.someEqual('role', 'system') || input.messages.countWhere('role', 'user') > 0",
        context
      );
      expect(result2).toBe(true);
      
      // Test complex combination
      const result3 = evaluator.evaluate(
        "input.messages.someEqual('role', 'user') && input.messages.countWhere('role', 'assistant') == 2 && input.forkResults.someEqual('status', 'FAILED')",
        context
      );
      expect(result3).toBe(true);
    });
    
    it("should work in ternary expressions", () => {
      // Test ternary with array method comparison
      // We have 3 user messages, so countWhere('role', 'user') == 3
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') > 2 ? 'long_conversation' : 'short_conversation'",
        context
      );
      expect(result).toBe('long_conversation');
      
      // Test with a condition that should be false
      const result2 = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') > 10 ? 'very_long' : 'normal'",
        context
      );
      expect(result2).toBe('normal');
      
      // Test ternary with boolean array method
      const result3 = evaluator.evaluate(
        "input.messages.someEqual('role', 'system') ? 'has_system' : 'no_system'",
        context
      );
      expect(result3).toBe('no_system');
      
      // Test nested ternary
      const result4 = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') > 10 ? 'very_long' : (input.messages.countWhere('role', 'user') > 2 ? 'long' : 'short')",
        context
      );
      expect(result4).toBe('long');
    });
  });
  
  describe("Type Validation", () => {
    it("should warn when comparing incompatible types with numeric operators", () => {
      const testContext = {
        ...context,
        variables: {
          threshold: 'not a number'
        }
      };
      
      // This should trigger a type warning and return false
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') > variables.threshold",
        testContext
      );
      expect(result).toBe(false);
    });
    
    it("should handle numeric comparisons correctly", () => {
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') >= 3",
        context
      );
      expect(result).toBe(true);
    });
    
    it("should handle equality comparisons with type warnings", () => {
      // Comparing number with string should work but may log a warning
      const result = evaluator.evaluate(
        "input.messages.countWhere('role', 'user') == 3",
        context
      );
      expect(result).toBe(true);
    });
  });
  
  // Phase 3.1: Aggregation Functions
  describe("Aggregation Functions (Phase 3.1)", () => {
    let numericContext: EvaluationContext;
    
    beforeEach(() => {
      numericContext = {
        input: {
          results: [
            { score: 85, duration: 120 },
            { score: 92, duration: 95 },
            { score: 78, duration: 150 },
            { score: 95, duration: 80 },
            { score: 88, duration: 110 }
          ]
        },
        output: {},
        variables: {}
      };
    });
    
    describe("sum", () => {
      it("should sum numeric property values", () => {
        const result = evaluator.evaluate(
          "input.results.sum('score')",
          numericContext
        );
        expect(result).toBe(438); // 85 + 92 + 78 + 95 + 88
      });
      
      it("should return 0 for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { results: [] }
        };
        
        const result = evaluator.evaluate(
          "input.results.sum('score')",
          emptyContext
        );
        expect(result).toBe(0);
      });
      
      it("should ignore non-numeric values", () => {
        const mixedContext = {
          ...numericContext,
          input: {
            results: [
              { score: 10 },
              { score: 'not a number' },
              { score: 20 }
            ]
          }
        };
        
        const result = evaluator.evaluate(
          "input.results.sum('score')",
          mixedContext
        );
        expect(result).toBe(30); // Only numeric values
      });
    });
    
    describe("avg", () => {
      it("should calculate average of numeric property values", () => {
        const result = evaluator.evaluate(
          "input.results.avg('score')",
          numericContext
        );
        expect(result).toBe(87.6); // 438 / 5
      });
      
      it("should return 0 for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { results: [] }
        };
        
        const result = evaluator.evaluate(
          "input.results.avg('score')",
          emptyContext
        );
        expect(result).toBe(0);
      });
      
      it("should ignore non-numeric values in calculation", () => {
        const mixedContext = {
          ...numericContext,
          input: {
            results: [
              { score: 10 },
              { score: 'invalid' },
              { score: 20 }
            ]
          }
        };
        
        const result = evaluator.evaluate(
          "input.results.avg('score')",
          mixedContext
        );
        expect(result).toBe(15); // (10 + 20) / 2
      });
    });
    
    describe("min", () => {
      it("should find minimum value", () => {
        const result = evaluator.evaluate(
          "input.results.min('score')",
          numericContext
        );
        expect(result).toBe(78);
      });
      
      it("should return null for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { results: [] }
        };
        
        const result = evaluator.evaluate(
          "input.results.min('score')",
          emptyContext
        );
        expect(result).toBeNull();
      });
      
      it("should ignore non-numeric values", () => {
        const mixedContext = {
          ...numericContext,
          input: {
            results: [
              { score: 10 },
              { score: 'text' },
              { score: 20 }
            ]
          }
        };
        
        const result = evaluator.evaluate(
          "input.results.min('score')",
          mixedContext
        );
        expect(result).toBe(10);
      });
    });
    
    describe("max", () => {
      it("should find maximum value", () => {
        const result = evaluator.evaluate(
          "input.results.max('score')",
          numericContext
        );
        expect(result).toBe(95);
      });
      
      it("should return null for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { results: [] }
        };
        
        const result = evaluator.evaluate(
          "input.results.max('score')",
          emptyContext
        );
        expect(result).toBeNull();
      });
      
      it("should ignore non-numeric values", () => {
        const mixedContext = {
          ...numericContext,
          input: {
            results: [
              { score: 10 },
              { score: 'text' },
              { score: 20 }
            ]
          }
        };
        
        const result = evaluator.evaluate(
          "input.results.max('score')",
          mixedContext
        );
        expect(result).toBe(20);
      });
    });
  });
  
  // Phase 3.2: Comparison-based Filters
  describe("Comparison-based Filters (Phase 3.2)", () => {
    let numericContext: EvaluationContext;
    
    beforeEach(() => {
      numericContext = {
        input: {
          scores: [
            { value: 10 },
            { value: 25 },
            { value: 50 },
            { value: 75 },
            { value: 100 }
          ]
        },
        output: {},
        variables: {}
      };
    });
    
    describe("someGreaterThan", () => {
      it("should return true if any value is greater than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.someGreaterThan('value', 50)",
          numericContext
        );
        expect(result).toBe(true); // 75 and 100 are > 50
      });
      
      it("should return false if no value is greater than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.someGreaterThan('value', 100)",
          numericContext
        );
        expect(result).toBe(false);
      });
      
      it("should return false for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { scores: [] }
        };
        
        const result = evaluator.evaluate(
          "input.scores.someGreaterThan('value', 50)",
          emptyContext
        );
        expect(result).toBe(false);
      });
    });
    
    describe("someLessThan", () => {
      it("should return true if any value is less than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.someLessThan('value', 50)",
          numericContext
        );
        expect(result).toBe(true); // 10 and 25 are < 50
      });
      
      it("should return false if no value is less than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.someLessThan('value', 10)",
          numericContext
        );
        expect(result).toBe(false);
      });
      
      it("should return false for empty arrays", () => {
        const emptyContext = {
          ...numericContext,
          input: { scores: [] }
        };
        
        const result = evaluator.evaluate(
          "input.scores.someLessThan('value', 50)",
          emptyContext
        );
        expect(result).toBe(false);
      });
    });
    
    describe("everyGreaterThan", () => {
      it("should return true if all values are greater than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.everyGreaterThan('value', 5)",
          numericContext
        );
        expect(result).toBe(true); // All values > 5
      });
      
      it("should return false if not all values are greater than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.everyGreaterThan('value', 20)",
          numericContext
        );
        expect(result).toBe(false); // 10 is not > 20
      });
      
      it("should return true for empty arrays (vacuously true)", () => {
        const emptyContext = {
          ...numericContext,
          input: { scores: [] }
        };
        
        const result = evaluator.evaluate(
          "input.scores.everyGreaterThan('value', 50)",
          emptyContext
        );
        expect(result).toBe(true);
      });
    });
    
    describe("everyLessThan", () => {
      it("should return true if all values are less than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.everyLessThan('value', 150)",
          numericContext
        );
        expect(result).toBe(true); // All values < 150
      });
      
      it("should return false if not all values are less than threshold", () => {
        const result = evaluator.evaluate(
          "input.scores.everyLessThan('value', 80)",
          numericContext
        );
        expect(result).toBe(false); // 100 is not < 80
      });
      
      it("should return true for empty arrays (vacuously true)", () => {
        const emptyContext = {
          ...numericContext,
          input: { scores: [] }
        };
        
        const result = evaluator.evaluate(
          "input.scores.everyLessThan('value', 50)",
          emptyContext
        );
        expect(result).toBe(true);
      });
    });
  });
  
  // Phase 3.4: Array Transformation Methods
  describe("Array Transformation Methods (Phase 3.4)", () => {
    let transformContext: EvaluationContext;
    
    beforeEach(() => {
      transformContext = {
        input: {
          users: [
            { name: 'Alice', role: 'admin' },
            { name: 'Bob', role: 'user' },
            { name: 'Charlie', role: 'user' },
            { name: 'Diana', role: 'moderator' }
          ],
          numbers: [1, 2, 3, 2, 4, 3, 5]
        },
        output: {},
        variables: {}
      };
    });
    
    describe("map", () => {
      it("should extract property values into array", () => {
        const result = evaluator.evaluate(
          "input.users.map('name')",
          transformContext
        );
        expect(result).toEqual(['Alice', 'Bob', 'Charlie', 'Diana']);
      });
      
      it("should return empty array for empty input", () => {
        const emptyContext = {
          ...transformContext,
          input: { users: [], numbers: [] }
        };
        
        const result = evaluator.evaluate(
          "input.users.map('name')",
          emptyContext
        );
        expect(result).toEqual([]);
      });
      
      it("should handle missing properties as undefined", () => {
        const incompleteContext = {
          ...transformContext,
          input: {
            users: [
              { name: 'Alice' },
              { role: 'user' }, // Missing name
              { name: 'Charlie' }
            ],
            numbers: []
          }
        };
        
        const result = evaluator.evaluate(
          "input.users.map('name')",
          incompleteContext
        );
        expect(result).toEqual(['Alice', undefined, 'Charlie']);
      });
    });
    
    describe("distinct", () => {
      it("should return unique values", () => {
        // Test distinct on roles
        const distinctResult = evaluator.evaluate(
          "input.users.distinct('role')",
          transformContext
        );
        expect(distinctResult).toEqual(['admin', 'user', 'moderator']);
      });
      
      it("should return empty array for empty input", () => {
        const emptyContext = {
          ...transformContext,
          input: { users: [], numbers: [] }
        };
        
        const result = evaluator.evaluate(
          "input.users.distinct('role')",
          emptyContext
        );
        expect(result).toEqual([]);
      });
    });
    
    describe("first", () => {
      it("should return first element", () => {
        const result = evaluator.evaluate(
          "input.users.first()",
          transformContext
        );
        expect(result).toEqual({ name: 'Alice', role: 'admin' });
      });
      
      it("should return null for empty arrays", () => {
        const emptyContext = {
          ...transformContext,
          input: { users: [], numbers: [] }
        };
        
        const result = evaluator.evaluate(
          "input.users.first()",
          emptyContext
        );
        expect(result).toBeNull();
      });
    });
    
    describe("last", () => {
      it("should return last element", () => {
        const result = evaluator.evaluate(
          "input.users.last()",
          transformContext
        );
        expect(result).toEqual({ name: 'Diana', role: 'moderator' });
      });
      
      it("should return null for empty arrays", () => {
        const emptyContext = {
          ...transformContext,
          input: { users: [], numbers: [] }
        };
        
        const result = evaluator.evaluate(
          "input.users.last()",
          emptyContext
        );
        expect(result).toBeNull();
      });
    });
  });
});
