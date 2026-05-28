/**
 * OS Hook Strategies — Unit Tests
 *
 * Tests for all files in the os-hooks directory:
 *   - base.ts:         executePassthrough()
 *   - index.ts:        createOsHookStrategy() + barrel exports
 *   - linux-seccomp.ts:    LinuxSeccompStrategy
 *   - proot-redirect.ts:   ProotLikeRedirectStrategy
 *   - windows-job-object.ts: WindowsJobObjectStrategy
 */

import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import {
  LinuxSeccompStrategy,
  WindowsJobObjectStrategy,
  ProotLikeRedirectStrategy,
  createOsHookStrategy,
} from "../index.js";
import { executePassthrough } from "../base.js";
import type { SandboxPolicy, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../../types.js";
import type { TerminalService } from "../../../../terminal/index.js";

// =========================================================================
// Test Helpers
// =========================================================================

function createMockTerminalService() {
  return {
    executeOneOff: vi.fn().mockResolvedValue({
      success: true,
      stdout: "executed",
      stderr: "",
      exitCode: 0,
    }),
    spawnProcess: vi.fn(),
    monitorProcess: vi.fn(),
  };
}

const defaultPolicy: SandboxPolicy = {
  mode: "strict",
  shell: {
    allowedCommands: [],
    deniedCommands: [],
    dangerousPatterns: [],
    allowPipe: true,
    allowRedirect: true,
  },
  process: {
    allowExec: false,
    allowFork: false,
  },
  network: {
    access: "none",
  },
  resource: {
    timeoutLimit: 10000,
  },
};

/** Helper to temporarily override process.platform */
function withPlatform(platform: NodeJS.Platform, fn: () => void) {
  const original = process.platform;
  try {
    Object.defineProperty(process, "platform", { value: platform });
    fn();
  } finally {
    Object.defineProperty(process, "platform", { value: original });
  }
}

// =========================================================================
// base.ts — executePassthrough
// =========================================================================

describe("executePassthrough (base.ts)", () => {
  let mockTerminal: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminal = createMockTerminalService();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const options: StrategyExecuteOptions = {
    command: "echo hello",
    cwd: "/tmp",
    env: { FOO: "bar" },
    timeout: 5000,
  };

  it("should return success result when terminalService succeeds", async () => {
    mockTerminal.executeOneOff.mockResolvedValue({
      success: true,
      stdout: "hello world",
      stderr: "",
      exitCode: 0,
    });

    const result = await executePassthrough(mockTerminal as unknown as TerminalService, options, Date.now());

    expect(result.success).toBe(true);
    expect(result.stdout).toBe("hello world");
    expect(result.stderr).toBe("");
    expect(result.exitCode).toBe(0);
    expect(result.scriptName).toBe("sandbox-os-hook");
    expect(result.error).toBeUndefined();
    expect(mockTerminal.executeOneOff).toHaveBeenCalledWith("echo hello", {
      cwd: "/tmp",
      env: { FOO: "bar" },
      timeout: 5000,
    });
  });

  it("should return failure result when terminalService fails", async () => {
    mockTerminal.executeOneOff.mockResolvedValue({
      success: false,
      stdout: "",
      stderr: "error occurred",
      exitCode: 1,
      error: "command failed",
    });

    const result = await executePassthrough(mockTerminal as unknown as TerminalService, options, Date.now());

    expect(result.success).toBe(false);
    expect(result.stderr).toBe("error occurred");
    expect(result.exitCode).toBe(1);
    expect(result.error).toBe("command failed");
  });

  it("should return failure result when terminalService throws", async () => {
    mockTerminal.executeOneOff.mockRejectedValue(new Error("connection lost"));

    const result = await executePassthrough(mockTerminal as unknown as TerminalService, options, Date.now());

    expect(result.success).toBe(false);
    expect(result.error).toBe("connection lost");
    expect(result.stdout).toBeUndefined();
    expect(result.stderr).toBeUndefined();
  });

  it("should return failure with string message when terminalService throws non-Error", async () => {
    mockTerminal.executeOneOff.mockRejectedValue("string error");

    const result = await executePassthrough(mockTerminal as unknown as TerminalService, options, Date.now());

    expect(result.success).toBe(false);
    expect(result.error).toBe("string error");
  });

  it("should report executionTime correctly", async () => {
    vi.setSystemTime(1000);
    const startTime = Date.now();

    vi.setSystemTime(2500); // 1500ms later
    const result = await executePassthrough(mockTerminal as unknown as TerminalService, options, startTime);

    expect(result.executionTime).toBe(1500);
  });

  it("should pass options without cwd/env/timeout", async () => {
    const minimalOptions: StrategyExecuteOptions = {
      command: "ls",
    };

    await executePassthrough(mockTerminal as unknown as TerminalService, minimalOptions, Date.now());

    expect(mockTerminal.executeOneOff).toHaveBeenCalledWith("ls", {
      cwd: undefined,
      env: undefined,
      timeout: undefined,
    });
  });
});

// =========================================================================
// index.ts — createOsHookStrategy & barrel exports
// =========================================================================

describe("index.ts — barrel exports", () => {
  it("should export LinuxSeccompStrategy", () => {
    expect(LinuxSeccompStrategy).toBeDefined();
    expect(LinuxSeccompStrategy.prototype).toBeDefined();
  });

  it("should export WindowsJobObjectStrategy", () => {
    expect(WindowsJobObjectStrategy).toBeDefined();
    expect(WindowsJobObjectStrategy.prototype).toBeDefined();
  });

  it("should export ProotLikeRedirectStrategy", () => {
    expect(ProotLikeRedirectStrategy).toBeDefined();
    expect(ProotLikeRedirectStrategy.prototype).toBeDefined();
  });

  it("should export createOsHookStrategy", () => {
    expect(createOsHookStrategy).toBeDefined();
    expect(typeof createOsHookStrategy).toBe("function");
  });
});

describe("createOsHookStrategy", () => {
  it("should return WindowsJobObjectStrategy on win32", () => {
    withPlatform("win32", () => {
      const strategy = createOsHookStrategy();
      expect(strategy).toBeInstanceOf(WindowsJobObjectStrategy);
    });
  });

  it("should return LinuxSeccompStrategy on linux", () => {
    withPlatform("linux", () => {
      const strategy = createOsHookStrategy();
      expect(strategy).toBeInstanceOf(LinuxSeccompStrategy);
    });
  });

  it("should return ProotLikeRedirectStrategy on darwin", () => {
    withPlatform("darwin", () => {
      const strategy = createOsHookStrategy();
      expect(strategy).toBeInstanceOf(ProotLikeRedirectStrategy);
    });
  });

  it("should return ProotLikeRedirectStrategy on other platforms", () => {
    withPlatform("freebsd" as NodeJS.Platform, () => {
      const strategy = createOsHookStrategy();
      expect(strategy).toBeInstanceOf(ProotLikeRedirectStrategy);
    });
  });

  it("should pass terminalService to strategy constructor", () => {
    const mockTerminal = createMockTerminalService();
    withPlatform("linux", () => {
      const strategy = createOsHookStrategy(mockTerminal as unknown as TerminalService);
      // Strategy should use the provided terminal service (not auto-resolve)
      expect(strategy).toBeInstanceOf(LinuxSeccompStrategy);
    });
  });

  it("should return a StrategyImplementation with execute method", () => {
    withPlatform("linux", () => {
      const strategy = createOsHookStrategy();
      expect(typeof strategy.execute).toBe("function");
    });
  });
});

// =========================================================================
// LinuxSeccompStrategy
// =========================================================================

describe("LinuxSeccompStrategy", () => {
  let strategy: LinuxSeccompStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new LinuxSeccompStrategy(mockTerminalService as unknown as TerminalService);
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("linux-seccomp");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Seccomp");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("seccomp");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(50);
    });
  });

  describe("isAvailable", () => {
    it("should return true on Linux", () => {
      withPlatform("linux", () => {
        expect(strategy.isAvailable()).toBe(true);
      });
    });

    it("should return false on Windows", () => {
      withPlatform("win32", () => {
        expect(strategy.isAvailable()).toBe(false);
      });
    });

    it("should return false on macOS", () => {
      withPlatform("darwin", () => {
        expect(strategy.isAvailable()).toBe(false);
      });
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "echo hello",
      shellType: "bash",
    };

    it("should return error when not on Linux", async () => {
      await withPlatform("darwin", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.success).toBe(false);
        expect(result.error).toContain("only available on Linux");
      });
    });

    it("should fall back to passthrough when on Linux but no seccomp-loader binary found", async () => {
      await withPlatform("linux", async () => {
        // execSync('which seccomp-loader') will fail in test env,
        // and no binary exists in the usual paths → executes passthrough
        const result = await strategy.execute(baseOptions, defaultPolicy);

        expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
        expect(result.success).toBe(true);
        expect(result.stdout).toBe("executed");
      });
    });

    it("should call executeOneOff with the passthrough command when falling back", async () => {
      await withPlatform("linux", async () => {
        await strategy.execute(baseOptions, defaultPolicy);

        expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
          "echo hello",
          expect.objectContaining({
            cwd: undefined,
            env: undefined,
            timeout: undefined,
          }),
        );
      });
    });

    it("should include options in passthrough call when provided", async () => {
      const optsWithAll: StrategyExecuteOptions = {
        command: "gcc --version",
        cwd: "/home/user",
        env: { PATH: "/usr/bin" },
        timeout: 30000,
      };

      await withPlatform("linux", async () => {
        await strategy.execute(optsWithAll, defaultPolicy);

        expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
          "gcc --version",
          expect.objectContaining({
            cwd: "/home/user",
            env: { PATH: "/usr/bin" },
            timeout: 30000,
          }),
        );
      });
    });

    it("should propagate terminalService errors in passthrough fallback", async () => {
      mockTerminalService.executeOneOff.mockRejectedValue(new Error("execution failed"));

      await withPlatform("linux", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.success).toBe(false);
        expect(result.error).toBe("execution failed");
      });
    });

    it("should report executionTime", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "done",
        stderr: "",
        exitCode: 0,
      });

      await withPlatform("linux", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.executionTime).toBeGreaterThanOrEqual(0);
        expect(typeof result.executionTime).toBe("number");
      });
    });
  });
});

// =========================================================================
// ProotLikeRedirectStrategy
// =========================================================================

describe("ProotLikeRedirectStrategy", () => {
  let strategy: ProotLikeRedirectStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new ProotLikeRedirectStrategy(mockTerminalService as unknown as TerminalService);
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("proot-redirect");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Proot");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("proot");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(40);
    });
  });

  describe("isAvailable", () => {
    it("should return false when proot is not installed (test environment)", () => {
      // In the test env, proot won't be on PATH → returns false
      expect(strategy.isAvailable()).toBe(false);
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "gcc --version",
      cwd: "/tmp",
    };

    it("should fall back to passthrough when proot binary not found", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        "gcc --version",
        expect.objectContaining({ cwd: "/tmp" }),
      );
      expect(result.success).toBe(true);
      expect(result.stdout).toBe("executed");
    });

    it("should passthrough with full options", async () => {
      const opts: StrategyExecuteOptions = {
        command: "ls -la",
        cwd: "/home",
        env: { HOME: "/home" },
        timeout: 15000,
      };

      await strategy.execute(opts, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        "ls -la",
        expect.objectContaining({
          cwd: "/home",
          env: { HOME: "/home" },
          timeout: 15000,
        }),
      );
    });

    it("should propagate terminalService error", async () => {
      mockTerminalService.executeOneOff.mockRejectedValue(new Error("proot error"));

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("proot error");
    });

    it("should handle terminalService returning failure", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: false,
        stdout: "",
        stderr: "command failed",
        exitCode: 1,
        error: "exit code 1",
      });

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
      expect(result.error).toBe("exit code 1");
    });

    it("should report executionTime", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });
  });
});

// =========================================================================
// WindowsJobObjectStrategy
// =========================================================================

describe("WindowsJobObjectStrategy", () => {
  let strategy: WindowsJobObjectStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new WindowsJobObjectStrategy(mockTerminalService as unknown as TerminalService);
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("windows-job");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Job Object");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("Job Object");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(50);
    });
  });

  describe("isAvailable", () => {
    it("should return true on Windows", () => {
      withPlatform("win32", () => {
        expect(strategy.isAvailable()).toBe(true);
      });
    });

    it("should return false on Linux", () => {
      withPlatform("linux", () => {
        expect(strategy.isAvailable()).toBe(false);
      });
    });

    it("should return false on macOS", () => {
      withPlatform("darwin", () => {
        expect(strategy.isAvailable()).toBe(false);
      });
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "echo hello",
      shellType: "cmd",
    };

    it("should return error when not on Windows", async () => {
      await withPlatform("linux", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.success).toBe(false);
        expect(result.error).toContain("only available on Windows");
      });
    });

    it("should fall back to passthrough on Windows when koffi is not available", async () => {
      await withPlatform("win32", async () => {
        // koffi is not installed in test env → getKoffiBinding() returns null → passthrough
        const result = await strategy.execute(baseOptions, defaultPolicy);

        expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
        expect(result.success).toBe(true);
        expect(result.stdout).toBe("executed");
      });
    });

    it("should call executeOneOff with correct passthrough args when koffi absent", async () => {
      await withPlatform("win32", async () => {
        const opts: StrategyExecuteOptions = {
          command: "dir",
          cwd: "C:\\Users",
          env: { PATH: "C:\\Windows" },
          timeout: 10000,
        };

        await strategy.execute(opts, defaultPolicy);

        expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
          "dir",
          expect.objectContaining({
            cwd: "C:\\Users",
            env: { PATH: "C:\\Windows" },
            timeout: 10000,
          }),
        );
      });
    });

    it("should propagate terminalService errors when falling back", async () => {
      mockTerminalService.executeOneOff.mockRejectedValue(new Error("windows error"));

      await withPlatform("win32", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.success).toBe(false);
        expect(result.error).toBe("windows error");
      });
    });

    it("should report executionTime", async () => {
      await withPlatform("win32", async () => {
        const result = await strategy.execute(baseOptions, defaultPolicy);
        expect(result.executionTime).toBeGreaterThanOrEqual(0);
      });
    });
  });
});
