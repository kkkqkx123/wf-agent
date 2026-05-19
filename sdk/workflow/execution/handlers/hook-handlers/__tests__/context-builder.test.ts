/**
 * Workflow Hook Context Builder Unit Tests
 *
 * Tests for the Workflow hook context builder with node output support.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { EvaluationContext, NodeExecutionResult } from "@wf-agent/types";
import {
  buildHookEvaluationContext,
  convertToEvaluationContext,
  type HookEvaluationContext,
} from "../context-builder.js";
import type { HookExecutionContext } from "../hook-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { StaticNode } from "@wf-agent/types";

describe("Workflow Hook Context Builder", () => {
  let mockEntity: WorkflowExecutionEntity;
  let mockNode: StaticNode;
  let mockResult: NodeExecutionResult;

  beforeEach(() => {
    // Create mock workflow execution entity
    mockEntity = {
      getInput: vi.fn().mockReturnValue({ userName: "testUser", age: 25 }),
      getOutput: vi.fn().mockReturnValue({ finalResult: "done" }),
      messageHistoryManager: {
        getMessages: vi.fn().mockReturnValue([
          { role: "user", content: "Hello" },
          { role: "assistant", content: "Hi there!" },
        ]),
      },
      variableStateManager: {
        getAllVariables: vi.fn().mockReturnValue({ var1: "value1", var2: 42 }),
      },
      getExecution: vi.fn().mockReturnValue({
        output: { finalResult: "done" },
      }),
    } as any;

    // Create mock node
    mockNode = {
      id: "test-node-1",
      type: "SCRIPT",
      name: "Test Script",
      config: { scriptName: "test_script" },
      metadata: { description: "Test node" },
      hooks: [],
    } as StaticNode;

    // Create mock result WITH output
    mockResult = {
      nodeId: "test-node-1",
      nodeType: "SCRIPT",
      status: "COMPLETED",
      step: 1,
      executionTime: 100,
      output: { transformedData: [1, 2, 3], count: 3 },
    };
  });

  describe("buildHookEvaluationContext", () => {
    it("should build context with workflow input", () => {
      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: mockNode,
        result: mockResult,
      };

      const hookContext = buildHookEvaluationContext(context);

      expect(hookContext.workflowInput).toEqual({ userName: "testUser", age: 25 });
      expect(mockEntity.getInput).toHaveBeenCalled();
    });

    it("should include node output from execution result", () => {
      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: mockNode,
        result: mockResult,
      };

      const hookContext = buildHookEvaluationContext(context);

      expect(hookContext.nodeOutput).toBeDefined();
      expect(hookContext.nodeOutput).toEqual({ transformedData: [1, 2, 3], count: 3 });
    });

    it("should handle missing result gracefully", () => {
      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: mockNode,
        result: undefined,
      };

      const hookContext = buildHookEvaluationContext(context);

      expect(hookContext.nodeOutput).toBeUndefined();
      expect(hookContext.status).toBe("PENDING");
      expect(hookContext.executionTime).toBe(0);
    });

    it("should expose message history", () => {
      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: mockNode,
        result: mockResult,
      };

      const hookContext = buildHookEvaluationContext(context);

      expect(hookContext.messages).toBeDefined();
      expect(Array.isArray(hookContext.messages)).toBe(true);
      expect(hookContext.messages.length).toBe(2);
    });

    it("should include all required fields", () => {
      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: mockNode,
        result: mockResult,
      };

      const hookContext = buildHookEvaluationContext(context);

      expect(hookContext).toHaveProperty("workflowInput");
      expect(hookContext).toHaveProperty("nodeOutput");
      expect(hookContext).toHaveProperty("output");
      expect(hookContext).toHaveProperty("messages");
      expect(hookContext).toHaveProperty("status");
      expect(hookContext).toHaveProperty("executionTime");
      expect(hookContext).toHaveProperty("variables");
      expect(hookContext).toHaveProperty("config");
      expect(hookContext).toHaveProperty("metadata");
    });
  });

  describe("convertToEvaluationContext", () => {
    it("should expose workflow input in input namespace", () => {
      const hookContext: HookEvaluationContext = {
        workflowInput: { userName: "testUser" },
        nodeOutput: { result: "data" },
        output: { final: "result" },
        messages: [{ role: "user", content: "test" }],
        status: "COMPLETED",
        executionTime: 100,
        variables: { var1: "value1" },
        config: {},
      };

      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input).toBeDefined();
      expect(evalContext.input.userName).toBe("testUser");
      expect(evalContext.input.messages).toBeDefined();
      expect((evalContext.input.messages as any[]).length).toBe(1);
    });

    it("should expose node output in output namespace", () => {
      const hookContext: HookEvaluationContext = {
        workflowInput: {},
        nodeOutput: { transformedData: [1, 2, 3] },
        output: { finalResult: "done" },
        messages: [],
        status: "COMPLETED",
        executionTime: 100,
        variables: {},
        config: {},
      };

      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.output).toBeDefined();
      expect(evalContext.output.result).toEqual({ finalResult: "done" });
      expect(evalContext.output.nodeOutput).toEqual({ transformedData: [1, 2, 3] });
    });

    it("should handle undefined node output", () => {
      const hookContext: HookEvaluationContext = {
        workflowInput: {},
        nodeOutput: undefined,
        output: {},
        messages: [],
        status: "COMPLETED",
        executionTime: 0,
        variables: {},
        config: {},
      };

      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.output.nodeOutput).toBeUndefined();
    });

    it("should preserve all evaluation context fields", () => {
      const hookContext: HookEvaluationContext = {
        workflowInput: { param1: "value1" },
        nodeOutput: { data: "test" },
        output: { result: "final" },
        messages: [{ role: "user", content: "msg" }],
        status: "FAILED",
        executionTime: 200,
        error: new Error("Test error"),
        variables: { var1: 123 },
        config: { testConfig: true },
        metadata: { meta: "data" },
      };

      const evalContext = convertToEvaluationContext(hookContext);

      expect(evalContext.input.param1).toBe("value1");
      expect(evalContext.input.messages.length).toBe(1);
      expect(evalContext.output.result).toEqual({ result: "final" });
      expect(evalContext.output.nodeOutput).toEqual({ data: "test" });
      expect(evalContext.output.status).toBe("FAILED");
      expect(evalContext.output.executionTime).toBe(200);
      expect(evalContext.output.error).toBeInstanceOf(Error);
      expect(evalContext.variables.var1).toBe(123);
    });
  });

  describe("Integration: Full Hook Context Flow", () => {
    it("should support complete workflow hook scenario", () => {
      // Simulate a SCRIPT node that transforms data
      const scriptResult: NodeExecutionResult = {
        nodeId: "transform_node",
        nodeType: "SCRIPT",
        status: "COMPLETED",
        step: 2,
        executionTime: 150,
        output: { 
          transformedCount: 10,
          errors: 0,
          data: [/* transformed data */]
        },
      };

      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: { ...mockNode, type: "SCRIPT" },
        result: scriptResult,
      };

      const hookContext = buildHookEvaluationContext(context);
      const evalContext = convertToEvaluationContext(hookContext);

      // Verify we can access node output for condition evaluation
      expect(evalContext.output.nodeOutput).toBeDefined();
      expect((evalContext.output.nodeOutput as any).transformedCount).toBe(10);
      
      // Verify workflow input is accessible
      expect(evalContext.input.userName).toBe("testUser");
      
      // Verify messages are accessible
      expect((evalContext.input.messages as any[]).length).toBe(2);
    });

    it("should support SUBGRAPH node output checking", () => {
      const subgraphResult: NodeExecutionResult = {
        nodeId: "subgraph_node",
        nodeType: "SUBGRAPH",
        status: "COMPLETED",
        step: 3,
        executionTime: 500,
        output: {
          executionResult: {
            output: { analysisComplete: true },
            status: "COMPLETED",
          },
          duration: 450,
        },
      };

      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: { ...mockNode, type: "SUBGRAPH" },
        result: subgraphResult,
      };

      const hookContext = buildHookEvaluationContext(context);
      const evalContext = convertToEvaluationContext(hookContext);

      // Verify subgraph execution status can be checked
      expect(evalContext.output.nodeOutput).toBeDefined();
      expect((evalContext.output.nodeOutput as any).executionResult.status).toBe("COMPLETED");
    });

    it("should support LLM node content checking", () => {
      const llmResult: NodeExecutionResult = {
        nodeId: "llm_node",
        nodeType: "LLM",
        status: "COMPLETED",
        step: 4,
        executionTime: 2000,
        output: {
          content: "This is the LLM response",
          toolCalls: [],
        },
      };

      const context: HookExecutionContext = {
        workflowExecutionEntity: mockEntity,
        node: { ...mockNode, type: "LLM" },
        result: llmResult,
      };

      const hookContext = buildHookEvaluationContext(context);
      const evalContext = convertToEvaluationContext(hookContext);

      // Verify LLM content is accessible
      expect(evalContext.output.nodeOutput).toBeDefined();
      expect((evalContext.output.nodeOutput as any).content).toBe("This is the LLM response");
    });
  });
});
