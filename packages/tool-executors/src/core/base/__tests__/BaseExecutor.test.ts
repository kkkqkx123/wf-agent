/**
 * BaseExecutor Unit Tests
 */

import { describe, it, expect, vi, beforeEach, type Mock } from "vitest";
import { BaseExecutor } from "../BaseExecutor.js";
import { ParameterValidator } from "../ParameterValidator.js";
import { RetryStrategy } from "../RetryStrategy.js";
import { TimeoutController } from "../TimeoutController.js";
import type { Tool, ToolExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, TimeoutError, NetworkError } from "@wf-agent/types";

// Create a specific executor implementation for testing.
class TestExecutor extends BaseExecutor {
  private mockExecute: Mock;

  constructor(
    mockExecute: Mock,
    validator?: ParameterValidator,
    retryStrategy?: RetryStrategy,
    timeoutController?: TimeoutController,
  ) {
    super(validator, retryStrategy, timeoutController);
    this.mockExecute = mockExecute;
  }

  protected async doExecute(
    tool: Tool,
    parameters: Record<string, any>,
    threadId?: string,
  ): Promise<any> {
    return this.mockExecute(tool, parameters, threadId);
  }

  getExecutorType(): string {
    return "test-executor";
  }
}

// Create an auxiliary function for the testing tool
const createTool = (params: { properties: Record<string, any>; required?: string[] }): Tool => ({
  id: "test-tool",
  name: "Test Tool",
  type: "STATELESS",
  description: "A test tool",
  parameters: {
    type: "object",
    properties: params.properties,
    required: params.required || [],
  },
});

describe("BaseExecutor", () => {
  let mockExecute: Mock;
  let executor: TestExecutor;
  let tool: Tool;

  beforeEach(() => {
    mockExecute = vi.fn().mockResolvedValue({ data: "success" });
    executor = new TestExecutor(mockExecute);
    tool = createTool({
      properties: {
        input: { type: "string" },
      },
      required: ["input"],
    });
  });

  describe("constructor", () => {
    it("Instances should be created using the default component", () => {
      const defaultExecutor = new TestExecutor(mockExecute);
      expect(defaultExecutor).toBeInstanceOf(BaseExecutor);
    });

    it("Instances should be created using custom components", () => {
      const validator = new ParameterValidator();
      const retryStrategy = RetryStrategy.createNoRetry();
      const timeoutController = new TimeoutController(5000);

      const customExecutor = new TestExecutor(
        mockExecute,
        validator,
        retryStrategy,
        timeoutController,
      );

      expect(customExecutor).toBeInstanceOf(BaseExecutor);
    });
  });

  describe("execute", () => {
    describe("Successful implementation", () => {
      it("Should execute successfully and return results", async () => {
        mockExecute.mockResolvedValue({ data: "result" });

        const result = await executor.execute(tool, { input: "test" });

        expect(result.success).toBe(true);
        expect(result.result).toEqual({ data: "result" });
        expect(result.retryCount).toBe(0);
        expect(result.executionTime).toBeGreaterThanOrEqual(0);
      });

      it("The correct parameters should be passed to doExecute.", async () => {
        mockExecute.mockResolvedValue("success");

        await executor.execute(tool, { input: "test-value" }, {}, "thread-123");

        expect(mockExecute).toHaveBeenCalledWith(tool, { input: "test-value" }, "thread-123");
      });
    });

    describe("parameter verification", () => {
      it("Should throw an error if parameter validation fails", async () => {
        // Missing required parameters
        try {
          await executor.execute(tool, {});
          expect.fail("Should have thrown RuntimeValidationError");
        } catch (error) {
          expect(error).toBeInstanceOf(RuntimeValidationError);
        }
      });

      it("Should throw an error if the parameter type is wrong", async () => {
        // Type error
        try {
          await executor.execute(tool, { input: 123 });
          expect.fail("Should have thrown RuntimeValidationError");
        } catch (error) {
          expect(error).toBeInstanceOf(RuntimeValidationError);
        }
      });
    });

    describe("timeout handling", () => {
      it("Should return a failure result after a timeout", async () => {
        mockExecute.mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        const result = await executor.execute(tool, { input: "test" }, { timeout: 50 });

        expect(result.success).toBe(false);
        expect(result.error).toContain("timeout");
      });

      it("The timeout in the options should be used", async () => {
        mockExecute.mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        const result = await executor.execute(tool, { input: "test" }, { timeout: 50 });

        expect(result.success).toBe(false);
      });
    });

    describe("Retesting mechanism", () => {
      it("Should be retried on failure", async () => {
        mockExecute
          .mockRejectedValueOnce(new NetworkError("Failed 1"))
          .mockRejectedValueOnce(new NetworkError("Failed 2"))
          .mockResolvedValue({ data: "success" });

        const result = await executor.execute(
          tool,
          { input: "test" },
          { retries: 3, retryDelay: 10, exponentialBackoff: false },
        );

        expect(result.success).toBe(true);
        // retryCount Indicates the index of the last iteration when it failed
        // First failure (i=0), second failure (i=1), third success (i=2)
        // On success the return is updated from the last failure retryCount = 1
        expect(result.retryCount).toBe(1);
        expect(mockExecute).toHaveBeenCalledTimes(3);
      });

      it("Should return failure after the retry count is exhausted", async () => {
        mockExecute.mockRejectedValue(new NetworkError("Always fails"));

        const result = await executor.execute(
          tool,
          { input: "test" },
          { retries: 2, retryDelay: 10, exponentialBackoff: false },
        );

        expect(result.success).toBe(false);
        expect(result.retryCount).toBe(2);
        expect(mockExecute).toHaveBeenCalledTimes(3); // Initial + 2 retries
      });

      it("Exponential backoff should be used", async () => {
        mockExecute
          .mockRejectedValueOnce(new NetworkError("Failed 1"))
          .mockRejectedValueOnce(new NetworkError("Failed 2"))
          .mockResolvedValue({ data: "success" });

        const result = await executor.execute(
          tool,
          { input: "test" },
          { retries: 3, retryDelay: 10, exponentialBackoff: true },
        );

        expect(result.success).toBe(true);
      });

      it("Should fail immediately for non-retryable errors", async () => {
        const nonRetryableError = new Error("Non-retryable error");
        mockExecute.mockRejectedValue(nonRetryableError);

        const result = await executor.execute(
          tool,
          { input: "test" },
          { retries: 3, retryDelay: 10 },
        );

        expect(result.success).toBe(false);
        expect(mockExecute).toHaveBeenCalledTimes(1); // Do not retry.
      });
    });

    describe("abort signal", () => {
      it("Abort signals should be supported", async () => {
        const abortController = new AbortController();

        mockExecute.mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        const promise = executor.execute(
          tool,
          { input: "test" },
          { signal: abortController.signal, timeout: 5000 },
        );

        // Stop execution
        abortController.abort();

        const result = await promise;

        expect(result.success).toBe(false);
        expect(result.error).toContain("aborted");
      });
    });

    describe("implementation option", () => {
      it("The default option should be used", async () => {
        mockExecute.mockResolvedValue("success");

        const result = await executor.execute(tool, { input: "test" });

        expect(result.success).toBe(true);
      });

      it("Default options should be overridden", async () => {
        mockExecute.mockResolvedValue("success");

        const result = await executor.execute(
          tool,
          { input: "test" },
          {
            timeout: 60000,
            retries: 5,
            retryDelay: 2000,
            exponentialBackoff: false,
          },
        );

        expect(result.success).toBe(true);
      });
    });

    describe("error handling", () => {
      it("Errors that are not of the Error type should be handled correctly", async () => {
        mockExecute.mockRejectedValue("string error");

        const result = await executor.execute(tool, { input: "test" }, { retries: 0 });

        expect(result.success).toBe(false);
        expect(result.error).toBe("string error");
      });

      it("Should contain execution time", async () => {
        mockExecute.mockImplementation(
          () => new Promise(resolve => setTimeout(() => resolve("done"), 10)),
        );

        const result = await executor.execute(tool, { input: "test" });

        expect(result.executionTime).toBeGreaterThanOrEqual(0);
      });
    });
  });

  describe("validateParameters", () => {
    it("Valid parameters should be validated", () => {
      expect(() => executor.validateParameters(tool, { input: "test" })).not.toThrow();
    });

    it("Should throw an error on invalid arguments", () => {
      expect(() => executor.validateParameters(tool, {})).toThrow(RuntimeValidationError);
    });
  });

  describe("getExecutorType", () => {
    it("The actuator type should be returned", () => {
      expect(executor.getExecutorType()).toBe("test-executor");
    });
  });
});
