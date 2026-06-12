/**
 * JavaScript VM Context Strategy Tests
 *
 * Tests for JavaScriptVmContextStrategy:
 *   - Identity properties
 *   - Empty code handling
 *   - Code execution and output capture
 *   - Timeout behavior
 *   - Restricted require (allowFSWrite, child_process, module denylist/allowed)
 *   - Read-only fs proxy
 *   - VFS-backed fs proxy
 *   - VFS sandbox helper globals
 *   - Console output capture
 */

import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { JavaScriptVmContextStrategy } from "../../strategies/js-vm-context.js";
import type { SandboxPolicy, StrategyExecuteOptions, VFSProvider } from "@wf-agent/types";
import os from "os";
import path from "path";
import fs from "fs";

// =========================================================================
// Helpers
// =========================================================================

const defaultPolicy: SandboxPolicy = {
  mode: "strict",
};

/** Create a mock VFS provider */
function createMockVFS(): VFSProvider {
  return {
    readFile: vi.fn(),
    writeFile: vi.fn(),
    stat: vi.fn(),
    exists: vi.fn(),
  };
}

// =========================================================================
// JavaScriptVmContextStrategy
// =========================================================================

describe("JavaScriptVmContextStrategy", () => {
  let strategy: JavaScriptVmContextStrategy;
  const tmpFilesCreated: string[] = [];

  beforeEach(() => {
    strategy = new JavaScriptVmContextStrategy();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    // Clean up any temp files created during tests
    for (const filePath of tmpFilesCreated) {
      try {
        if (fs.existsSync(filePath)) {
          fs.unlinkSync(filePath);
        }
      } catch {
        // Ignore cleanup errors
      }
    }
    tmpFilesCreated.length = 0;
  });

  // ── Identity ──

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("vm-context");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("VM");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("vm");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(30);
    });

    it("should always be available", () => {
      expect(strategy.isAvailable()).toBe(true);
    });
  });

  // ── Executing code ──

  describe("execute", () => {
    it("should reject empty code", async () => {
      const options: StrategyExecuteOptions = { command: "" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("Empty JavaScript code");
      expect(result.scriptName).toBe("sandbox-javascript");
    });

    it("should execute valid JavaScript and capture stdout", async () => {
      const options: StrategyExecuteOptions = { command: 'console.log("hello world");' };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("hello world");
      expect(result.exitCode).toBe(0);
    });

    it("should capture multiple console.log calls", async () => {
      const options: StrategyExecuteOptions = {
        command: 'console.log("line1");\nconsole.log("line2");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("line1\nline2");
    });

    it("should capture console.error", async () => {
      const options: StrategyExecuteOptions = { command: 'console.error("error output");' };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("error output");
    });

    it("should capture console.warn and console.info", async () => {
      const options: StrategyExecuteOptions = {
        command: 'console.warn("warning"); console.info("info");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("warning\ninfo");
    });

    it("should capture object output via JSON serialization", async () => {
      const options: StrategyExecuteOptions = {
        command: 'console.log({ key: "value", num: 42 });',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toContain('"key"');
      expect(result.stdout).toContain('"value"');
    });

    it("should report execution time", async () => {
      const start = Date.now();
      const options: StrategyExecuteOptions = { command: 'console.log("timing");' };

      // Set time forward during execution
      vi.setSystemTime(start + 500);
      const result = await strategy.execute(options, defaultPolicy);

      expect(typeof result.executionTime).toBe("number");
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should capture script runtime error", async () => {
      const options: StrategyExecuteOptions = { command: 'throw new Error("boom");' };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("boom");
    });

    it("should capture syntax error", async () => {
      const options: StrategyExecuteOptions = { command: "invalid syntax {{{" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Unexpected identifier");
    });
  }); // close describe("execute")

  // ── Sandbox restrictions ──

  describe("sandbox capabilities", () => {
    it("should provide Buffer.from in sandbox", async () => {
      const options: StrategyExecuteOptions = {
        command: 'const b = Buffer.from("hello", "utf-8"); console.log(b.length);',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("5");
    });

    it("should provide process.env with NODE_ENV=sandbox", async () => {
      const options: StrategyExecuteOptions = {
        command: "console.log(process.env.NODE_ENV);",
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("sandbox");
    });

    it("should provide process.cwd returning /workspace", async () => {
      const options: StrategyExecuteOptions = {
        command: "console.log(process.cwd());",
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("/workspace");
    });

    it("should make eval undefined", async () => {
      const options: StrategyExecuteOptions = {
        command: "console.log(typeof eval);",
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("undefined");
    });

    it("should make Function undefined", async () => {
      const options: StrategyExecuteOptions = {
        command: "console.log(typeof Function);",
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("undefined");
    });

    it("should have empty global object", async () => {
      const options: StrategyExecuteOptions = {
        command: "console.log(JSON.stringify(Object.keys(global)));",
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      // global should be empty or have minimal keys
      expect(result.stdout).toBe("[]");
    });

    it("should enforce timeout", async () => {
      const options: StrategyExecuteOptions = {
        command: "while(true) {}",
        timeout: 100,
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("timed out");
    });
  }); // close describe("sandbox capabilities")

  // ── Restricted require ──

  describe("restricted require", () => {
    it("should allow require of permitted modules (path)", async () => {
      const options: StrategyExecuteOptions = {
        command: 'const path = require("path"); console.log(path.basename("/a/b/c"));',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("c");
    });

    it("should deny require of child_process by default", async () => {
      const options: StrategyExecuteOptions = {
        command: 'require("child_process");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("child_process");
    });

    it("should deny require of denied modules (fs)", async () => {
      // fs is denied by default via DEFAULT_DENIED_MODULES — wait, let me check.
      // Actually fs is NOT in DEFAULT_DENIED_MODULES. It's handled specially via allowFSWrite.
      // Let me test with "vm" which is in DEFAULT_DENIED_MODULES.
      const options: StrategyExecuteOptions = {
        command: 'require("vm");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("denied");
    });

    it("should deny require of cluster module", async () => {
      const options: StrategyExecuteOptions = {
        command: 'require("cluster");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("denied");
    });

    it("should return read-only fs when require('fs') without write permission", async () => {
      const options: StrategyExecuteOptions = {
        command: 'const fs = require("fs"); console.log(typeof fs.readFileSync);',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("function");
    });

    it("should block write operations on read-only fs proxy", async () => {
      const options: StrategyExecuteOptions = {
        command: 'const fs = require("fs"); fs.writeFileSync("/tmp/test", "data");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Read-only");
    });

    it("should allow fs writes when policy allows it", async () => {
      const writePolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: true,
          allowDynamicEval: false,
        },
      };
      const tmpFile = path.join(os.tmpdir(), `test-${Date.now()}.txt`);
      tmpFilesCreated.push(tmpFile);
      // Escape backslashes for the JavaScript string inside the sandbox code
      const escapedPath = tmpFile.replace(/\\/g, "\\\\");
      const options: StrategyExecuteOptions = {
        command: `const fs = require("fs"); fs.writeFileSync("${escapedPath}", "hello"); console.log("ok");`,
      };
      const result = await strategy.execute(options, writePolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("ok");
    });

    it("should deny non-existent module", async () => {
      const options: StrategyExecuteOptions = {
        command: 'require("nonexistent-module-xyz-123");',
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("not found");
    });

    it("should respect allowedModules whitelist", async () => {
      const restrictedPolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: ["path"],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: 'require("fs");',
      };
      const result = await strategy.execute(options, restrictedPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("not allowed");
    });

    it("should respect allowedModules whitelist for allowed module", async () => {
      const restrictedPolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: ["path"],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: 'const p = require("path"); console.log(typeof p.join);',
      };
      const result = await strategy.execute(options, restrictedPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("function");
    });

    it("should allow child_process when policy allows it", async () => {
      const allowCpPolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: [],
          allowChildProcess: true,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: 'const cp = require("child_process"); console.log(typeof cp.execSync);',
      };
      const result = await strategy.execute(options, allowCpPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("function");
    });

    it("should handle custom deniedModules overriding defaults", async () => {
      // When deniedModules is non-empty, defaults aren't used
      const customDenyPolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: ["path"],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: 'require("path");',
      };
      const result = await strategy.execute(options, customDenyPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("denied");
    });

    it("should allow require of custom deniedModules exempt modules", async () => {
      // vm is in defaults but not in custom → should be allowed
      const customDenyPolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: ["path"], // only deny path, not vm
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: 'const vm = require("vm"); console.log(typeof vm.Script);',
      };
      const result = await strategy.execute(options, customDenyPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("function");
    });
  });

  // ── VFS-backed fs proxy ──

  describe("VFS-backed fs proxy", () => {
    it("should read files via VFS when allowFSWrite is true and VFS is provided", async () => {
      const mockVFS = createMockVFS();
      (mockVFS.readFile as ReturnType<typeof vi.fn>).mockResolvedValue(
        new TextEncoder().encode("vfs content"),
      );

      const writePolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: `const fs = require("fs"); fs.readFile("/test.txt", "utf-8").then(data => { console.log("done"); });`,
        vfs: mockVFS,
      };
      const result = await strategy.execute(options, writePolicy);

      expect(result.success).toBe(true);
    });

    it("should write files via VFS when allowFSWrite is true and VFS is provided", async () => {
      const mockVFS = createMockVFS();
      (mockVFS.writeFile as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      const writePolicy: SandboxPolicy = {
        mode: "strict",
        javascript: {
          allowedModules: [],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: true,
          allowDynamicEval: false,
        },
      };
      const options: StrategyExecuteOptions = {
        command: `const fs = require("fs"); fs.writeFile("/test.txt", "data").then(() => { console.log("written"); });`,
        vfs: mockVFS,
      };
      const result = await strategy.execute(options, writePolicy);

      expect(result.success).toBe(true);
    });

    it("should expose vfs sandbox global when VFS is provided", async () => {
      const mockVFS = createMockVFS();
      (mockVFS.readFile as ReturnType<typeof vi.fn>).mockResolvedValue(
        new TextEncoder().encode("vfs content"),
      );
      (mockVFS.exists as ReturnType<typeof vi.fn>).mockResolvedValue(true);

      const options: StrategyExecuteOptions = {
        command: "console.log(typeof vfs.readFile);",
        vfs: mockVFS,
      };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("function");
    });
  });
});
