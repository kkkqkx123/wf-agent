/**
 * ShellExecutor 测试
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ShellExecutor } from "../ShellExecutor.js";
import type { Script } from "@wf-agent/types";

describe("ShellExecutor", () => {
  let executor: ShellExecutor;

  beforeEach(() => {
    executor = new ShellExecutor();
  });

  it("The simple shell script should execute successfully.", async () => {
    const script: Script = {
      id: "test-1",
      name: "test-script",
      type: "SHELL",
      description: "Test script",
      content: 'echo "Hello, World!"',
      options: {
        timeout: 5000,
      },
    };

    const result = await executor.execute(script);

    expect(result.success).toBe(true);
    expect(result.stdout).toContain("Hello, World!");
    expect(result.exitCode).toBe(0);
  });

  it("Environment variables should be supported.", async () => {
    const script: Script = {
      id: "test-2",
      name: "test-env-script",
      type: "SHELL",
      description: "Test environment variables",
      content: "echo $TEST_VAR",
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

  it("Execution errors should be handled accordingly.", async () => {
    const script: Script = {
      id: "test-3",
      name: "test-error-script",
      type: "SHELL",
      description: "Test error handling",
      content: "exit 1",
      options: {
        timeout: 5000,
      },
    };

    const result = await executor.execute(script);

    expect(result.success).toBe(false);
    expect(result.exitCode).toBe(1);
  });

  it("The supported script types should be returned.", () => {
    const types = executor.getSupportedTypes();
    expect(types).toEqual(["SHELL"]);
  });

  it("The executor type should be returned.", () => {
    const type = executor.getExecutorType();
    expect(type).toBe("SHELL");
  });
});
