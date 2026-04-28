/**
 * CommandLineExecutor 测试
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { CommandLineExecutor } from "../CommandLineExecutor.js";
import type { Script, ScriptType } from "@wf-agent/types";
import type {
  ExecutionContext,
  ExecutionOutput,
  ExecutorConfig,
  ExecutorType,
} from "../../types.js";
import { EventEmitter } from "events";

// Simulate the `spawn` method of `child_process`
vi.mock("child_process", () => ({
  spawn: vi.fn(),
}));

import { spawn } from "child_process";

// Create a specific command line executor for testing purposes
class TestCommandLineExecutor extends CommandLineExecutor<"SHELL"> {
  constructor(config?: Omit<ExecutorConfig, "type"> & { type: "SHELL" }) {
    super(config);
  }

  protected getCommandLineConfig(script: Script) {
    return {
      command: "sh",
      args: ["-c", script.content || ""],
      shell: false,
      windowsHide: false,
    };
  }
}

// Creating a simulated child process
function createMockChildProcess(exitCode: number = 0, stdout: string = "", stderr: string = "") {
  const mockChild = new EventEmitter() as any;
  mockChild.stdout = new EventEmitter();
  mockChild.stderr = new EventEmitter();
  mockChild.kill = vi.fn();

  // Analog Data Output
  if (stdout) {
    setTimeout(() => {
      mockChild.stdout.emit("data", Buffer.from(stdout));
    }, 10);
  }

  if (stderr) {
    setTimeout(() => {
      mockChild.stderr.emit("data", Buffer.from(stderr));
    }, 10);
  }

  // Simulating process exit
  setTimeout(() => {
    mockChild.emit("close", exitCode);
  }, 20);

  return mockChild;
}

describe("CommandLineExecutor", () => {
  let executor: TestCommandLineExecutor;
  let mockScript: Script;

  beforeEach(() => {
    vi.clearAllMocks();
    executor = new TestCommandLineExecutor();
    mockScript = {
      id: "test-1",
      name: "test-script",
      type: "SHELL",
      description: "Test script",
      content: 'echo "Hello, World!"',
      options: {},
    };
  });

  describe("Constructor", () => {
    it("The instance should be created using the default configuration.", () => {
      const testExecutor = new TestCommandLineExecutor();
      expect(testExecutor).toBeInstanceOf(CommandLineExecutor);
      expect(testExecutor.getExecutorType()).toBe("SHELL");
    });

    it("Instances should be created using custom configurations.", () => {
      const config: Omit<ExecutorConfig, "type"> & { type: "SHELL" } = {
        type: "SHELL",
        timeout: 60000,
        maxRetries: 5,
        retryDelay: 2000,
      };
      const testExecutor = new TestCommandLineExecutor(config);
      expect(testExecutor).toBeInstanceOf(CommandLineExecutor);
    });
  });

  describe("doExecute", () => {
    it("The command should execute successfully.", async () => {
      const mockChild = createMockChildProcess(0, "Hello, World!", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      const result = await executor.execute(mockScript);

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("Hello, World!");
      expect(result.exitCode).toBe(0);
      expect(spawn).toHaveBeenCalledWith(
        "sh",
        ["-c", 'echo "Hello, World!"'],
        expect.objectContaining({
          env: expect.any(Object),
          cwd: expect.any(String),
          stdio: ["pipe", "pipe", "pipe"],
          shell: false,
          windowsHide: false,
        }),
      );
    });

    it("Failed execution cases should be handled.", async () => {
      const mockChild = createMockChildProcess(1, "", "Error occurred");
      vi.mocked(spawn).mockReturnValue(mockChild);

      const result = await executor.execute(mockScript);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Error occurred");
    });

    it("The empty script content should be handled accordingly.", async () => {
      const emptyScript: Script = {
        ...mockScript,
        content: "",
      };

      const result = await executor.execute(emptyScript);

      expect(result.success).toBe(false);
      expect(result.error).toContain("empty");
    }, 10000);

    it("Environment variables should be supported.", async () => {
      const scriptWithEnv: Script = {
        ...mockScript,
        options: {
          environment: {
            TEST_VAR: "test-value",
          },
        },
      };

      const mockChild = createMockChildProcess(0, "test-value", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(scriptWithEnv);

      expect(spawn).toHaveBeenCalledWith(
        "sh",
        ["-c", 'echo "Hello, World!"'],
        expect.objectContaining({
          env: expect.objectContaining({
            TEST_VAR: "test-value",
          }),
        }),
      );
    });

    it("Environment variables in the execution context should be supported.", async () => {
      const context: ExecutionContext = {
        environment: {
          CONTEXT_VAR: "context-value",
        },
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(mockScript, {}, context);

      expect(spawn).toHaveBeenCalledWith(
        "sh",
        ["-c", 'echo "Hello, World!"'],
        expect.objectContaining({
          env: expect.objectContaining({
            CONTEXT_VAR: "context-value",
          }),
        }),
      );
    });

    it("The working directory should be supported.", async () => {
      const scriptWithCwd: Script = {
        ...mockScript,
        options: {
          workingDirectory: "/tmp",
        },
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(scriptWithCwd);

      expect(spawn).toHaveBeenCalledWith(
        "sh",
        ["-c", 'echo "Hello, World!"'],
        expect.objectContaining({
          cwd: "/tmp",
        }),
      );
    });

    it("The working directory in the execution context should be supported.", async () => {
      const context: ExecutionContext = {
        workingDirectory: "/custom/path",
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(mockScript, {}, context);

      expect(spawn).toHaveBeenCalledWith(
        "sh",
        ["-c", 'echo "Hello, World!"'],
        expect.objectContaining({
          cwd: "/custom/path",
        }),
      );
    });

    it("The AbortSignal should be handled to stop the process.", async () => {
      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      const abortController = new AbortController();
      setTimeout(() => abortController.abort(), 5);

      const context: ExecutionContext = {
        signal: abortController.signal,
      };

      await executor.execute(mockScript, {}, context);

      expect(mockChild.kill).toHaveBeenCalledWith("SIGTERM");
    });

    it("Standard output should be collected.", async () => {
      const mockChild = createMockChildProcess(0, "Line 1\nLine 2\nLine 3", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      const result = await executor.execute(mockScript);

      expect(result.stdout).toBe("Line 1\nLine 2\nLine 3");
    });

    it("Standard errors should be collected.", async () => {
      const mockChild = createMockChildProcess(1, "", "Error line 1\nError line 2");
      vi.mocked(spawn).mockReturnValue(mockChild);

      const result = await executor.execute(mockScript);

      expect(result.stderr).toBe("Error line 1\nError line 2");
    });

    it("The situation where an exit code of null is encountered should be handled.", async () => {
      const mockChild = new EventEmitter() as any;
      mockChild.stdout = new EventEmitter();
      mockChild.stderr = new EventEmitter();
      mockChild.kill = vi.fn();

      vi.mocked(spawn).mockReturnValue(mockChild);

      setTimeout(() => {
        mockChild.emit("close", null);
      }, 20);

      const result = await executor.execute(mockScript);

      expect(result.exitCode).toBe(1);
      expect(result.success).toBe(false);
    });

    it("Multiple data events should be merged.", async () => {
      const mockChild = new EventEmitter() as any;
      mockChild.stdout = new EventEmitter();
      mockChild.stderr = new EventEmitter();
      mockChild.kill = vi.fn();

      vi.mocked(spawn).mockReturnValue(mockChild);

      // Simulate multiple data events
      setTimeout(() => mockChild.stdout.emit("data", Buffer.from("Part 1")), 5);
      setTimeout(() => mockChild.stdout.emit("data", Buffer.from("Part 2")), 10);
      setTimeout(() => mockChild.stdout.emit("data", Buffer.from("Part 3")), 15);
      setTimeout(() => mockChild.emit("close", 0), 20);

      const result = await executor.execute(mockScript);

      expect(result.stdout).toBe("Part 1Part 2Part 3");
    });
  });

  describe("getSupportedTypes", () => {
    it("The supported script types should be returned.", () => {
      const types = executor.getSupportedTypes();
      expect(types).toEqual(["SHELL"]);
    });
  });

  describe("getExecutorType", () => {
    it("The executor type should be returned.", () => {
      const type = executor.getExecutorType();
      expect(type).toBe("SHELL");
    });
  });

  describe("Environment Variable Priority", () => {
    it("The environment variables should be merged correctly (with context having the highest priority).", async () => {
      const script: Script = {
        ...mockScript,
        options: {
          environment: {
            VAR1: "script-value",
            VAR2: "script-value",
          },
        },
      };

      const context: ExecutionContext = {
        environment: {
          VAR2: "context-value",
          VAR3: "context-value",
        },
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(script, {}, context);

      const spawnCalls = vi.mocked(spawn).mock.calls;
      expect(spawnCalls.length).toBeGreaterThan(0);
      const spawnCall = spawnCalls[0]!;
      const env = spawnCall[2].env as Record<string, string>;

      expect(env["VAR1"]).toBe("script-value");
      expect(env["VAR2"]).toBe("context-value"); // Context Override Script
      expect(env["VAR3"]).toBe("context-value");
    });
  });

  describe("Working Directory Priority", () => {
    it("The working directory from the context should be used (with the highest priority).", async () => {
      const script: Script = {
        ...mockScript,
        options: {
          workingDirectory: "/script/path",
        },
      };

      const context: ExecutionContext = {
        workingDirectory: "/context/path",
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(script, {}, context);

      const spawnCalls = vi.mocked(spawn).mock.calls;
      expect(spawnCalls.length).toBeGreaterThan(0);
      const spawnCall = spawnCalls[0]!;
      expect(spawnCall[2].cwd).toBe("/context/path");
    });

    it("The working directory specified in the script should be used.", async () => {
      const script: Script = {
        ...mockScript,
        options: {
          workingDirectory: "/script/path",
        },
      };

      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(script);

      const spawnCalls = vi.mocked(spawn).mock.calls;
      expect(spawnCalls.length).toBeGreaterThan(0);
      const spawnCall = spawnCalls[0]!;
      expect(spawnCall[2].cwd).toBe("/script/path");
    });

    it("The current working directory should be used as the default value.", async () => {
      const mockChild = createMockChildProcess(0, "", "");
      vi.mocked(spawn).mockReturnValue(mockChild);

      await executor.execute(mockScript);

      const spawnCalls = vi.mocked(spawn).mock.calls;
      expect(spawnCalls.length).toBeGreaterThan(0);
      const spawnCall = spawnCalls[0]!;
      expect(spawnCall[2].cwd).toBe(process.cwd());
    });
  });
});
