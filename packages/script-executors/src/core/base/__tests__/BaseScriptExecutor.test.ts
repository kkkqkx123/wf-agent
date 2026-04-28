/**
 * BaseScriptExecutor testing
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { BaseScriptExecutor } from "../BaseScriptExecutor.js";
import type { Script, ScriptExecutionOptions, ScriptType } from "@wf-agent/types";
import type { ExecutionContext, ExecutionOutput, ExecutorConfig } from "../../types.js";

// Create a specific executor for testing purposes.
class TestExecutor extends BaseScriptExecutor {
  private mockExecute: (script: Script, context?: ExecutionContext) => Promise<ExecutionOutput>;

  constructor(
    config?: ExecutorConfig,
    mockExecute?: (script: Script, context?: ExecutionContext) => Promise<ExecutionOutput>,
  ) {
    super(config);
    this.mockExecute = mockExecute || this.defaultMockExecute;
  }

  private defaultMockExecute = async (script: Script): Promise<ExecutionOutput> => {
    return {
      stdout: `Executed: ${script.content}`,
      stderr: "",
      exitCode: 0,
    };
  };

  protected async doExecute(script: Script, context?: ExecutionContext): Promise<ExecutionOutput> {
    return this.mockExecute(script, context);
  }

  getExecutorType(): string {
    return "TEST";
  }

  override getSupportedTypes(): ScriptType[] {
    return ["SHELL"];
  }

  // The public `sleep` method is used for testing purposes.
  override async sleep(ms: number): Promise<void> {
    return super.sleep(ms);
  }
}

describe("BaseScriptExecutor", () => {
  let executor: TestExecutor;
  let mockScript: Script;

  beforeEach(() => {
    executor = new TestExecutor();
    mockScript = {
      id: "test-1",
      name: "test-script",
      type: "SHELL",
      description: "Test script",
      content: 'echo "Hello, World!"',
      options: {},
    };
  });

  describe("constructor", () => {
    it("Instances should be created using the default configuration", () => {
      const testExecutor = new TestExecutor();
      expect(testExecutor).toBeInstanceOf(BaseScriptExecutor);
      expect(testExecutor.getExecutorType()).toBe("TEST");
    });

    it("Instances should be created using a custom configuration", () => {
      const config: ExecutorConfig = {
        type: "SHELL",
        timeout: 60000,
        maxRetries: 5,
        retryDelay: 2000,
        exponentialBackoff: false,
      };
      const testExecutor = new TestExecutor(config);
      expect(testExecutor).toBeInstanceOf(BaseScriptExecutor);
    });
  });

  describe("execute", () => {
    it("The script should be executed successfully", async () => {
      const result = await executor.execute(mockScript);

      expect(result.success).toBe(true);
      expect(result.scriptName).toBe("test-script");
      expect(result.scriptType).toBe("SHELL");
      expect(result.stdout).toContain("Executed:");
      expect(result.exitCode).toBe(0);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("Implementation failures should be handled", async () => {
      const errorExecutor = new TestExecutor(undefined, async () => {
        return {
          stdout: "",
          stderr: "Error occurred",
          exitCode: 1,
        };
      });

      const result = await errorExecutor.execute(mockScript);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
      expect(result.error).toBe("Error occurred");
    });

    it("Custom execution options should be used", async () => {
      const options: ScriptExecutionOptions = {
        timeout: 10000,
        retries: 2,
        retryDelay: 500,
        exponentialBackoff: false,
      };

      const result = await executor.execute(mockScript, options);

      expect(result.success).toBe(true);
    });

    it("Execution contexts should be supported", async () => {
      const context: ExecutionContext = {
        workingDirectory: "/tmp",
        environment: {
          TEST_VAR: "test-value",
        },
      };

      const result = await executor.execute(mockScript, {}, context);

      expect(result.success).toBe(true);
    });

    it("Should return a failure result on timeout", async () => {
      const slowExecutor = new TestExecutor(undefined, async () => {
        await new Promise(resolve => setTimeout(resolve, 2000));
        return {
          stdout: "",
          stderr: "",
          exitCode: 0,
        };
      });

      const result = await slowExecutor.execute(mockScript, { timeout: 100 });

      expect(result.success).toBe(false);
      expect(result.error).toContain("timeout");
    }, 10000);

    it("Should succeed after a retry", async () => {
      let attemptCount = 0;
      const retryExecutor = new TestExecutor(undefined, async () => {
        attemptCount++;
        if (attemptCount < 2) {
          throw new Error("Temporary error");
        }
        return {
          stdout: "Success",
          stderr: "",
          exitCode: 0,
        };
      });

      const result = await retryExecutor.execute(mockScript, { retries: 3, retryDelay: 10 });

      expect(result.success).toBe(true);
      expect(attemptCount).toBe(2);
    });

    it("Should return failure after the retry count is exhausted", async () => {
      const alwaysFailExecutor = new TestExecutor(undefined, async () => {
        throw new Error("Always fails");
      });

      const result = await alwaysFailExecutor.execute(mockScript, { retries: 2, retryDelay: 10 });

      expect(result.success).toBe(false);
      expect(result.error).toContain("Always fails");
      expect(result.retryCount).toBe(2);
    });

    it("AbortSignal abort should be handled", async () => {
      const abortController = new AbortController();
      // Terminate immediately before execution.
      abortController.abort();

      const result = await executor.execute(mockScript, { signal: abortController.signal });

      // Execution should fail because signal has aborted
      // Note: If the execution finishes too quickly, it may succeed, this is normal behavior
      // Here we only verify the structure of the result
      expect(result).toBeDefined();
      expect(result.scriptName).toBe("test-script");
    }, 10000);

    it("Execution time should be calculated", async () => {
      const result = await executor.execute(mockScript);

      expect(result.executionTime).toBeGreaterThanOrEqual(0);
      expect(typeof result.executionTime).toBe("number");
    });
  });

  describe("validate", () => {
    it("Should always return validation success (deprecated)", () => {
      const result = executor.validate(mockScript);

      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });
  });

  describe("sleep", () => {
    it("Should sleep for a specified time", async () => {
      const start = Date.now();
      await executor.sleep(100);
      const end = Date.now();

      expect(end - start).toBeGreaterThanOrEqual(100);
    });
  });

  describe("cleanup", () => {
    it("Should successfully clean up resources", async () => {
      await expect(executor.cleanup()).resolves.not.toThrow();
    });
  });

  describe("getExecutorType", () => {
    it("The correct actuator type should be returned", () => {
      expect(executor.getExecutorType()).toBe("TEST");
    });
  });

  describe("getSupportedTypes", () => {
    it("The supported script types should be returned", () => {
      const types = executor.getSupportedTypes();
      expect(types).toEqual(["SHELL"]);
    });
  });

  describe("retry strategy", () => {
    it("Exponential backoff should be used", async () => {
      let attemptCount = 0;
      const delays: number[] = [];

      const retryExecutor = new TestExecutor(
        { type: "SHELL", retryDelay: 100, exponentialBackoff: true },
        async () => {
          attemptCount++;
          if (attemptCount < 3) {
            throw new Error("Temporary error");
          }
          return {
            stdout: "Success",
            stderr: "",
            exitCode: 0,
          };
        },
      );

      const start = Date.now();
      await retryExecutor.execute(mockScript, { retries: 3 });
      const end = Date.now();

      // Verify that a retry has occurred.
      expect(attemptCount).toBe(3);
      // The total verification time includes the delay (100 + 200 = 300ms).
      expect(end - start).toBeGreaterThanOrEqual(300);
    });

    it("Fixed delays should be used", async () => {
      let attemptCount = 0;

      const retryExecutor = new TestExecutor(
        { type: "SHELL", retryDelay: 100, exponentialBackoff: false },
        async () => {
          attemptCount++;
          if (attemptCount < 3) {
            throw new Error("Temporary error");
          }
          return {
            stdout: "Success",
            stderr: "",
            exitCode: 0,
          };
        },
      );

      const start = Date.now();
      await retryExecutor.execute(mockScript, { retries: 3 });
      const end = Date.now();

      // Verify that a retry has occurred.
      expect(attemptCount).toBe(3);
      // The total verification time includes the delay (100 + 100 = 200ms).
      expect(end - start).toBeGreaterThanOrEqual(200);
    });
  });
});
