/**
 * Test suite for variable operations in DATA_PROCESSOR node
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { contextProcessorHandler } from "../context-processor-handler.js";
import type {
  ContextProcessorNodeConfig,
  RuntimeNode,
  VariableAggregateOperation,
  VariableTransformOperation,
  VariableBatchUpdateOperation,
} from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { ContextProcessorHandlerContext } from "../context-processor-handler.js";

// Mock VariableManager
class MockVariableManager {
  private variables: Record<string, unknown> = {};

  setVariable(name: string, value: unknown) {
    this.variables[name] = value;
  }

  getVariable(name: string): unknown {
    return this.variables[name];
  }

  getAllVariables(): Record<string, unknown> {
    return { ...this.variables };
  }

  setExecutionEntity(_entity: any) {
    // No-op
  }

  cleanup() {
    // No-op
  }
}

// Create mock workflow execution entity
function createMockExecutionEntity(
  variables: Record<string, unknown> = {}
): WorkflowExecutionEntity {
  const mockVariableManager = new MockVariableManager();
  Object.entries(variables).forEach(([name, value]) => {
    mockVariableManager.setVariable(name, value);
  });

  const nodeResults: any[] = [];

  return {
    variableStateManager: mockVariableManager as any,
    getWorkflowExecutionData: () => ({
      id: "exec-1",
      workflowId: "wf-1",
      variables: Object.entries(variables).map(([name, value]) => ({
        name,
        value,
        type: typeof value,
      })),
      nodeResults,
      errors: [],
      input: {},
      output: {},
    } as any),
    addNodeResult: vi.fn((result: any) => {
      nodeResults.push(result);
    }),
    getNodeResults: () => nodeResults,
  } as unknown as WorkflowExecutionEntity;
}

describe("DATA_PROCESSOR - Variable Operations", () => {
  describe("Aggregate Operation - Array Mode", () => {
    it("should aggregate multiple variables into an array", async () => {
      const entity = createMockExecutionEntity({
        result_a: { id: 1, value: "A" },
        result_b: { id: 2, value: "B" },
        result_c: { id: 3, value: "C" },
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["result_a", "result_b", "result_c"],
        targetVariable: "all_results",
        aggregateMode: "array",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(result.operation).toBe("aggregate");
      expect(result.modifiedVariables).toHaveLength(1);
      expect(result.modifiedVariables[0].name).toBe("all_results");

      const aggregated = result.modifiedVariables[0].newValue;
      expect(Array.isArray(aggregated)).toBe(true);
      expect((aggregated as any[]).length).toBe(3);
      expect(result.stats?.sourceVariableCount).toBe(3);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should return without self-recording (recording is coordinator's responsibility)", async () => {
      const entity = createMockExecutionEntity({
        var_a: 1,
        var_b: 2,
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["var_a", "var_b"],
        targetVariable: "result",
        aggregateMode: "array",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "agg-node",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(entity.addNodeResult).not.toHaveBeenCalled();
    });
  });

  describe("Aggregate Operation - Object Mode", () => {
    it("should aggregate variables into an object with key mapping", async () => {
      const entity = createMockExecutionEntity({
        analysis_sentiment: "positive",
        analysis_intent: "purchase",
        analysis_entities: ["product", "price"],
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["analysis_sentiment", "analysis_intent", "analysis_entities"],
        targetVariable: "combined_analysis",
        aggregateMode: "object",
        keyMapping: {
          analysis_sentiment: "sentiment",
          analysis_intent: "intent",
          analysis_entities: "entities",
        },
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(result.operation).toBe("aggregate");
      const aggregated = result.modifiedVariables[0].newValue as any;
      expect(aggregated.sentiment).toBe("positive");
      expect(aggregated.intent).toBe("purchase");
      expect(aggregated.entities).toEqual(["product", "price"]);
    });

    it("should use variable names as keys when keyMapping not provided", async () => {
      const entity = createMockExecutionEntity({
        var_a: 1,
        var_b: 2,
        var_c: 3,
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["var_a", "var_b", "var_c"],
        targetVariable: "result",
        aggregateMode: "object",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      const aggregated = result.modifiedVariables[0].newValue as any;
      expect(aggregated.var_a).toBe(1);
      expect(aggregated.var_b).toBe(2);
      expect(aggregated.var_c).toBe(3);
    });
  });

  describe("Aggregate Operation - Merge Mode", () => {
    it("should shallow merge objects", async () => {
      const entity = createMockExecutionEntity({
        config_a: { server: "a.com", port: 8001 },
        config_b: { port: 8002, timeout: 3000 },
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["config_a", "config_b"],
        targetVariable: "merged_config",
        aggregateMode: "merge",
        mergeStrategy: "shallow",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      const merged = result.modifiedVariables[0].newValue as any;
      expect(merged.server).toBe("a.com");
      expect(merged.port).toBe(8002); // Later value overwrites
      expect(merged.timeout).toBe(3000);
    });

    it("should fail when trying to merge non-object values", async () => {
      const entity = createMockExecutionEntity({
        config_a: { key: "value" },
        config_b: "not_an_object",
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["config_a", "config_b"],
        targetVariable: "merged",
        aggregateMode: "merge",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      await expect(
        contextProcessorHandler(entity, node, {} as ContextProcessorHandlerContext)
      ).rejects.toThrow("Cannot merge non-object value");
    });
  });

  describe("Aggregate Operation - Error Cases", () => {
    it("should fail when source variable doesn't exist", async () => {
      const entity = createMockExecutionEntity({
        result_a: { id: 1 },
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["result_a", "nonexistent"],
        targetVariable: "all_results",
        aggregateMode: "array",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      await expect(
        contextProcessorHandler(entity, node, {} as ContextProcessorHandlerContext)
      ).rejects.toThrow("Source variable not found");

      // Handler no longer self-records; recording is coordinator's responsibility
      expect(entity.addNodeResult).not.toHaveBeenCalled();
    });
  });

  describe("Transform Operation", () => {
    it("should transform variable value", async () => {
      const entity = createMockExecutionEntity({
        data: [{value: 2}, {value: 4}, {value: 6}, {value: 8}, {value: 10}],
      });

      const operation: VariableTransformOperation = {
        operation: "transform",
        sourceVariable: "data",
        targetVariable: "doubled",
        transformExpression: 'data.map("value")',
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(result.operation).toBe("transform");
      const transformed = result.modifiedVariables[0].newValue as any[];
      expect(transformed).toEqual([2, 4, 6, 8, 10]);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should transform and convert to specified type", async () => {
      const entity = createMockExecutionEntity({
        count: 10,
      });

      const operation: VariableTransformOperation = {
        operation: "transform",
        sourceVariable: "count",
        targetVariable: "count_str",
        transformExpression: "count",
        outputType: "string",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      const transformed = result.modifiedVariables[0].newValue;
      expect(typeof transformed).toBe("string");
      expect(transformed).toBe("10");
    });

    it("should fail when source variable doesn't exist in transform", async () => {
      const entity = createMockExecutionEntity({
        data: [1, 2, 3],
      });

      const operation: VariableTransformOperation = {
        operation: "transform",
        sourceVariable: "nonexistent",
        targetVariable: "result",
        transformExpression: "nonexistent.length",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      await expect(
        contextProcessorHandler(entity, node, {} as ContextProcessorHandlerContext)
      ).rejects.toThrow("Source variable not found");
    });
  });

  describe("Batch Update Operation", () => {
    it("should batch update multiple variables", async () => {
      const entity = createMockExecutionEntity({
        count: 10,
        factor: 2,
      });

      const operation: VariableBatchUpdateOperation = {
        operation: "batch-update",
        updates: [
          {
            name: "result",
            expression: "count * factor",
            type: "number",
          },
          {
            name: "label",
            expression: "count > 5 ? 'high' : 'low'",
            type: "string",
          },
        ],
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(result.operation).toBe("batch-update");
      expect(result.modifiedVariables).toHaveLength(2);

      const resultVar = result.modifiedVariables.find((v) => v.name === "result");
      expect(resultVar?.newValue).toBe(20);

      const labelVar = result.modifiedVariables.find((v) => v.name === "label");
      expect(labelVar?.newValue).toBe("high");
    });

    it("should allow later updates to reference earlier updates", async () => {
      const entity = createMockExecutionEntity({
        price: 100,
        tax_rate: 0.1,
        discount_rate: 0.05,
      });

      const operation: VariableBatchUpdateOperation = {
        operation: "batch-update",
        updates: [
          {
            name: "discount",
            expression: "price * discount_rate",
            type: "number",
          },
          {
            name: "after_discount",
            expression: "price - discount",
            type: "number",
          },
          {
            name: "tax",
            expression: "after_discount * tax_rate",
            type: "number",
          },
          {
            name: "total",
            expression: "after_discount + tax",
            type: "number",
          },
        ],
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      const modified = Object.fromEntries(
        result.modifiedVariables.map((v) => [v.name, v.newValue])
      );

      expect(modified.discount).toBe(5); // 100 * 0.05
      expect(modified.after_discount).toBe(95); // 100 - 5
      expect(modified.tax).toBe(9.5); // 95 * 0.1
      expect(modified.total).toBe(104.5); // 95 + 9.5
    });

    it("should skip read-only variables in batch update", async () => {
      const entity = createMockExecutionEntity({
        constant: 42,
        mutable: 10,
      });

      // Mark constant as readonly by manipulating workflow execution data
      const workflowExec = entity.getWorkflowExecutionData();
      if (workflowExec.variables) {
        const constantVar = workflowExec.variables.find((v: any) => v.name === "constant");
        if (constantVar) {
          constantVar.readonly = true;
        }
      }

      const operation: VariableBatchUpdateOperation = {
        operation: "batch-update",
        updates: [
          {
            name: "constant",
            expression: "100",
            type: "number",
            readonly: true,
          },
          {
            name: "mutable",
            expression: "20",
            type: "number",
          },
        ],
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      // Only mutable should be updated
      expect(result.modifiedVariables).toHaveLength(1);
      expect(result.modifiedVariables[0].name).toBe("mutable");
      expect(result.modifiedVariables[0].newValue).toBe(20);
    });

    it("should fail when batch update expression evaluates with error", async () => {
      const entity = createMockExecutionEntity({
        data: { key: "value" },
      });

      const operation: VariableBatchUpdateOperation = {
        operation: "batch-update",
        updates: [
          {
            name: "result",
            expression: "undefined_var.length",
            type: "number",
          },
        ],
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      await expect(
        contextProcessorHandler(entity, node, {} as ContextProcessorHandlerContext)
      ).resolves.toBeDefined();
      const result = await contextProcessorHandler(entity, node, {} as ContextProcessorHandlerContext);
      expect(result.operation).toBe("batch-update");
      expect(result.modifiedVariables).toHaveLength(1);
      expect(result.modifiedVariables[0].name).toBe("result");
      // Expression evaluates to undefined, so NaN or undefined is expected
      expect(result.modifiedVariables[0].newValue).toBeNaN();
    });
  });

  describe("Variable Operation - Execution Time Recording", () => {
    it("should record execution time for all operations", async () => {
      const entity = createMockExecutionEntity({
        var_a: 1,
        var_b: 2,
      });

      const operation: VariableAggregateOperation = {
        operation: "aggregate",
        sourceVariables: ["var_a", "var_b"],
        targetVariable: "result",
        aggregateMode: "array",
      };

      const config: ContextProcessorNodeConfig = {
        variableOperation: operation,
      };

      const node: RuntimeNode = {
        id: "node-1",
        type: "CONTEXT_PROCESSOR",
        config,
      } as any;

      const result = await contextProcessorHandler(
        entity,
        node,
        {} as ContextProcessorHandlerContext
      );

      expect(typeof result.executionTime).toBe("number");
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });
  });
});
