/**
 * OS Hook Strategy Tests
 *
 * Tests for LinuxSeccompStrategy, WindowsJobObjectStrategy, and
 * ProotLikeRedirectStrategy.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  LinuxSeccompStrategy,
  WindowsJobObjectStrategy,
  ProotLikeRedirectStrategy,
} from "../../strategies/os-hooks/index.js";
import type { SandboxPolicy, StrategyExecuteOptions } from "@wf-agent/types";

// =========================================================================
// Test helpers
// =========================================================================

/** Stub for TerminalService used in strategy tests */
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

/** Minimal policy that enables OS hooks */
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

// =========================================================================
// LinuxSeccompStrategy Tests
// =========================================================================

describe("LinuxSeccompStrategy", () => {
  let strategy: LinuxSeccompStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new LinuxSeccompStrategy(mockTerminalService as any);
  });

  describe("isAvailable", () => {
    it("should return true on Linux", () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "linux" });
      expect(strategy.isAvailable()).toBe(true);
      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should return false on non-Linux", () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "win32" });
      expect(strategy.isAvailable()).toBe(false);
      Object.defineProperty(process, "platform", { value: originalPlatform });
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "echo hello",
      shellType: "bash",
    };

    it("should return error when not on Linux", async () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "darwin" });

      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.success).toBe(false);
      expect(result.error).toContain("only available on Linux");

      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should passthrough when seccomp-loader binary not found", async () => {
      // On non-Linux, the fallback path uses executeCommand which calls executeOneOff
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "linux" });

      // On Linux but no seccomp-loader binary — falls back to passthrough
      // We need to mock the findSeccompLoader to return null
      // Since it's a private method, we test via the strategy's behavior
      // by ensuring isAvailable returns true before calling execute
      const result = await strategy.execute(baseOptions, defaultPolicy);

      // On Linux without seccomp-loader, should fall back to passthrough (executeOneOff)
      // But findSeccompLoader uses execSync('which ...') which will fail in test env
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();

      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should set correct id and name", () => {
      expect(strategy.id).toBe("linux-seccomp");
      expect(strategy.name).toContain("Seccomp");
      expect(strategy.priority).toBe(50);
    });
  });
});

// =========================================================================
// WindowsJobObjectStrategy Tests
// =========================================================================

describe("WindowsJobObjectStrategy", () => {
  let strategy: WindowsJobObjectStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new WindowsJobObjectStrategy(mockTerminalService as any);
  });

  describe("isAvailable", () => {
    it("should return true on Windows", () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "win32" });
      expect(strategy.isAvailable()).toBe(true);
      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should return false on non-Windows", () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "linux" });
      expect(strategy.isAvailable()).toBe(false);
      Object.defineProperty(process, "platform", { value: originalPlatform });
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "echo hello",
      shellType: "cmd",
    };

    it("should return error when not on Windows", async () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "linux" });

      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.success).toBe(false);
      expect(result.error).toContain("only available on Windows");

      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should passthrough when koffi not available (on Windows)", async () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "win32" });

      // On Windows without koffi installed, falls back to executeOneOff passthrough
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();

      Object.defineProperty(process, "platform", { value: originalPlatform });
    });

    it("should set correct id and name", () => {
      expect(strategy.id).toBe("windows-job");
      expect(strategy.name).toContain("Job Object");
      expect(strategy.priority).toBe(50);
    });
  });
});

// =========================================================================
// ProotLikeRedirectStrategy Tests
// =========================================================================

describe("ProotLikeRedirectStrategy", () => {
  let strategy: ProotLikeRedirectStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new ProotLikeRedirectStrategy(mockTerminalService as any);
  });

  describe("isAvailable", () => {
    it("should return false when proot not installed", () => {
      // findProotBinary() will return null since proot isn't on PATH in CI
      expect(strategy.isAvailable()).toBe(false);
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "gcc --version",
      cwd: "/tmp",
    };

    it("should passthrough when proot binary not found", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);

      // Since proot is not available, should fall back to executeOneOff
      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        baseOptions.command,
        expect.objectContaining({ cwd: "/tmp" }),
      );
    });

    it("should set correct id and name", () => {
      expect(strategy.id).toBe("proot-redirect");
      expect(strategy.name).toContain("Proot");
      expect(strategy.priority).toBe(40);
    });
  });
});
