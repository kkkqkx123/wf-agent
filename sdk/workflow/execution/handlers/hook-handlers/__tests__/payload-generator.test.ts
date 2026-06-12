/**
 * Hook Payload Generator Unit Tests
 *
 * Tests for event payload generation and template resolution.
 * Uses the real renderTemplate with properly structured evalContext.
 */

import { describe, it, expect } from "vitest";
import type { NodeHook } from "@wf-agent/types";
import type { HookEvaluationContext } from "../context-builder.js";
import { generateHookEventData, resolvePayloadTemplate } from "../payload-generator.js";

/**
 * Build a realistic HookEvaluationContext for testing.
 * The renderTemplate function resolves paths (e.g. {{output.result}})
 * against the top-level keys of this context.
 */
function createEvalContext(overrides?: Partial<HookEvaluationContext>): HookEvaluationContext {
  return {
    workflowInput: { inputData: "test" },
    nodeOutput: { transformedData: [1, 2, 3] },
    output: {
      result: "42",
      status: "COMPLETED",
      isValid: "true",
      isError: "false",
      ratio: "3.14",
      name: "test-name",
      count: "0",
    },
    status: "COMPLETED",
    executionTime: 150,
    error: undefined,
    variables: { userName: "John", score: 100 },
    config: { key: "value" },
    metadata: { source: "unit-test" },
    messages: [],
    ...overrides,
  };
}

describe("generateHookEventData", () => {
  const mockEvalContext = createEvalContext();

  it("should return default event data when hook has no eventPayload", () => {
    const hook: NodeHook = {
      hookType: "AFTER_EXECUTE",
      eventName: "node.completed",
    };

    const result = generateHookEventData(hook, mockEvalContext);

    expect(result).toEqual({
      output: mockEvalContext.output,
      status: mockEvalContext.status,
      executionTime: mockEvalContext.executionTime,
      error: mockEvalContext.error,
      variables: mockEvalContext.variables,
      config: mockEvalContext.config,
      metadata: mockEvalContext.metadata,
    });
  });

  it("should resolve payload template when hook has eventPayload", () => {
    const hook: NodeHook = {
      hookType: "AFTER_EXECUTE",
      eventName: "node.completed",
      eventPayload: {
        result: "{{output.result}}",
        status: "{{output.status}}",
      },
    };

    const result = generateHookEventData(hook, mockEvalContext);

    expect(result).toEqual({
      result: 42,
      status: "COMPLETED",
    });
  });

  it("should handle empty eventPayload", () => {
    const hook: NodeHook = {
      hookType: "AFTER_EXECUTE",
      eventName: "node.completed",
      eventPayload: {},
    };

    const result = generateHookEventData(hook, mockEvalContext);

    expect(result).toEqual({});
  });

  it("should handle eventPayload with nested structure", () => {
    const hook: NodeHook = {
      hookType: "AFTER_EXECUTE",
      eventName: "node.completed",
      eventPayload: {
        summary: {
          result: "{{output.result}}",
          name: "{{output.name}}",
        },
        metadata: {
          source: "test",
        },
      },
    };

    const result = generateHookEventData(hook, mockEvalContext);

    expect(result).toEqual({
      summary: {
        result: 42,
        name: "test-name",
      },
      metadata: {
        source: "test",
      },
    });
  });
});

describe("resolvePayloadTemplate", () => {
  describe("variable resolution", () => {
    it("should resolve simple template variables", () => {
      const payload = { result: "{{output.result}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["result"]).toBe(42);
    });

    it("should recursively process nested objects", () => {
      const payload = {
        nested: {
          result: "{{output.result}}",
          static: "hello",
        },
      };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["nested"]).toEqual({ result: 42, static: "hello" });
    });

    it("should handle deeply nested structures", () => {
      const payload = {
        level1: {
          level2: {
            level3: "{{output.result}}",
          },
        },
      };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      const level1 = result["level1"] as Record<string, unknown>;
      const level2 = level1["level2"] as Record<string, unknown>;
      expect(level2["level3"]).toBe(42);
    });
  });

  describe("type conversion", () => {
    it("should convert 'true' string to boolean true", () => {
      const payload = { isValid: "{{output.isValid}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["isValid"]).toBe(true);
    });

    it("should convert 'false' string to boolean false", () => {
      const payload = { isError: "{{output.isError}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["isError"]).toBe(false);
    });

    it("should convert numeric string to number", () => {
      const payload = { ratio: "{{output.ratio}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["ratio"]).toBe(3.14);
    });

    it("should handle zero value correctly", () => {
      const payload = { count: "{{output.count}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["count"]).toBe(0);
    });
  });

  describe("value passthrough", () => {
    it("should pass through non-string, non-object values", () => {
      const payload = {
        numberValue: 100,
        booleanValue: true,
        nullValue: null,
      };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["numberValue"]).toBe(100);
      expect(result["booleanValue"]).toBe(true);
      expect(result["nullValue"]).toBeNull();
    });

    it("should keep non-template string as-is", () => {
      const payload = { name: "static-value" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["name"]).toBe("static-value");
    });
  });

  describe("edge cases", () => {
    it("should handle empty payload", () => {
      const result = resolvePayloadTemplate({}, createEvalContext());
      expect(result).toEqual({});
    });

    it("should resolve variables from variables namespace", () => {
      const payload = { user: "{{variables.userName}}" };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["user"]).toBe("John");
    });

    it("should handle mixed content in payload", () => {
      const payload = {
        staticString: "hello",
        dynamicString: "{{output.result}}",
        numberValue: 99,
        booleanValue: false,
        nullValue: null,
        nested: {
          key: "{{variables.userName}}",
        },
      };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["staticString"]).toBe("hello");
      expect(result["dynamicString"]).toBe(42);
      expect(result["numberValue"]).toBe(99);
      expect(result["booleanValue"]).toBe(false);
      expect(result["nullValue"]).toBeNull();
      const nested = result["nested"] as Record<string, unknown>;
      expect(nested["key"]).toBe("John");
    });

    it("should resolve template variables with inline {{}} in strings", () => {
      const payload = {
        message: "Result is: {{output.result}}",
      };
      const result = resolvePayloadTemplate(payload, createEvalContext());
      expect(result["message"]).toBe("Result is: 42");
    });
  });
});
