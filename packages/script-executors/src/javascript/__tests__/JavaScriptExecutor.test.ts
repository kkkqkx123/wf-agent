/**
 * JavaScriptExecutor testing
 */

import { describe, it, expect, beforeEach } from "vitest";
import { JavaScriptExecutor } from "../JavaScriptExecutor.js";
import type { Script } from "@wf-agent/types";

describe("JavaScriptExecutor", () => {
  let executor: JavaScriptExecutor;

  beforeEach(() => {
    executor = new JavaScriptExecutor();
  });

  it("The simple JavaScript script should execute successfully.", async () => {
    const script: Script = {
      id: "test-1",
      name: "test-script",
      type: "JAVASCRIPT",
      description: "Test script",
      content: 'console.log("Hello, World!");',
      options: {
        timeout: 5000,
      },
    };

    const result = await executor.execute(script);

    expect(result.success).toBe(true);
    expect(result.stdout).toContain("Hello, World!");
    expect(result.exitCode).toBe(0);
  });

  it("Environment variable access should be supported.", async () => {
    const script: Script = {
      id: "test-2",
      name: "test-env-script",
      type: "JAVASCRIPT",
      description: "Test environment variables",
      content: "console.log(process.env.TEST_VAR);",
      options: {
        timeout: 5000,
        environment: {
          TEST_VAR: "test-value",
        },
      },
    };

    const result = await executor.execute(script);

    expect(result.success).toBe(true);
    expect(result.stdout).toContain("test-value");
  });

  it("Execution errors should be handled.", async () => {
    const script: Script = {
      id: "test-3",
      name: "test-error-script",
      type: "JAVASCRIPT",
      description: "Test error handling",
      content: 'throw new Error("Test error");',
      options: {
        timeout: 5000,
      },
    };

    const result = await executor.execute(script);

    expect(result.success).toBe(false);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("Test error");
  });

  it("The supported script types should be returned.", () => {
    const types = executor.getSupportedTypes();
    expect(types).toEqual(["JAVASCRIPT"]);
  });

  it("The executor type should be returned.", () => {
    const type = executor.getExecutorType();
    expect(type).toBe("JAVASCRIPT");
  });
});
