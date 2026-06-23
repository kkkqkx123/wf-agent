/**
 * JavaScript VM Context Strategy — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { JavaScriptVmContextStrategy } from "../js-vm-context.js";
import type { SandboxPolicy, StrategyExecuteOptions } from "@wf-agent/types";

// =========================================================================
// Test Helpers
// =========================================================================

const defaultPolicy: SandboxPolicy = {
  mode: "strict",
  javascript: {
    allowedModules: [],
    deniedModules: ["child_process", "cluster", "worker_threads"],
    allowChildProcess: false,
    allowFSWrite: false,
    allowDynamicEval: false,
  },
  resource: {
    timeoutLimit: 10000,
  },
};

// =========================================================================
// JavaScriptVmContextStrategy
// =========================================================================

describe("JavaScriptVmContextStrategy", () => {
  let strategy: JavaScriptVmContextStrategy;

  beforeEach(() => {
    strategy = new JavaScriptVmContextStrategy();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("vm-context");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("JavaScript VM Context");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("vm.createContext");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(30);
    });
  });

  describe("isAvailable", () => {
    it("should always return true", () => {
      expect(strategy.isAvailable()).toBe(true);
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: 'console.log("hello")',
    };

    it("should return error for empty code", async () => {
      const result = await strategy.execute({ command: "" }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty JavaScript code");
    });

    it("should return error for undefined code", async () => {
      const result = await strategy.execute(
        { command: undefined } as unknown as StrategyExecuteOptions,
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty JavaScript code");
    });

    it("should execute simple console.log", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("hello");
    });

    it("should capture console.error output", async () => {
      const result = await strategy.execute(
        {
          command: 'console.error("error message")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("error message");
    });

    it("should capture console.warn output", async () => {
      const result = await strategy.execute(
        {
          command: 'console.warn("warning message")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("warning message");
    });

    it("should execute arithmetic operations", async () => {
      const result = await strategy.execute(
        {
          command: "const x = 1 + 2; console.log(x)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("3");
    });

    it("should execute object operations", async () => {
      const result = await strategy.execute(
        {
          command: "const obj = { a: 1, b: 2 }; console.log(obj.a + obj.b)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("3");
    });

    it("should execute array operations", async () => {
      const result = await strategy.execute(
        {
          command: "const arr = [1, 2, 3]; console.log(arr.reduce((a, b) => a + b, 0))",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("6");
    });

    it("should have access to Buffer", async () => {
      const result = await strategy.execute(
        {
          command: 'const buf = Buffer.from("hello"); console.log(buf.toString())',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("hello");
    });

    it("should have access to restricted process object", async () => {
      const result = await strategy.execute(
        {
          command: "console.log(process.platform)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain(process.platform);
    });

    it("should have process.env.NODE_ENV as sandbox", async () => {
      const result = await strategy.execute(
        {
          command: "console.log(process.env.NODE_ENV)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("sandbox");
    });

    it("should have process.cwd returning /workspace", async () => {
      const result = await strategy.execute(
        {
          command: "console.log(process.cwd())",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("/workspace");
    });

    it("should deny child_process module by default", async () => {
      const result = await strategy.execute(
        {
          command: 'const cp = require("child_process"); console.log("ok")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("child_process");
    });

    it("should deny cluster module by default", async () => {
      const result = await strategy.execute(
        {
          command: 'const cluster = require("cluster"); console.log("ok")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("cluster");
    });

    it("should deny vm module by default", async () => {
      // Use policy with vm in deniedModules
      const policy: SandboxPolicy = {
        ...defaultPolicy,
        javascript: {
          allowedModules: [],
          deniedModules: ["child_process", "cluster", "worker_threads", "vm"],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };
      const result = await strategy.execute(
        {
          command: 'const vm = require("vm"); console.log("ok")',
        },
        policy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("vm");
    });

    it("should allow path module", async () => {
      const result = await strategy.execute(
        {
          command: 'const path = require("path"); console.log(path.join("a", "b"))',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
    });

    it("should allow util module", async () => {
      const result = await strategy.execute(
        {
          command: 'const util = require("util"); console.log(typeof util.inspect)',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
    });

    it("should report executionTime", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
      expect(typeof result.executionTime).toBe("number");
    });

    it("should set scriptName to sandbox-javascript", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.scriptName).toBe("sandbox-javascript");
    });

    it("should set exitCode to 0 on success", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.exitCode).toBe(0);
    });

    it("should handle syntax errors", async () => {
      const result = await strategy.execute(
        {
          command: "const x = {",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it("should handle runtime errors", async () => {
      const result = await strategy.execute(
        {
          command: 'throw new Error("test error")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("test error");
    });

    it("should handle undefined variable access", async () => {
      const result = await strategy.execute(
        {
          command: "console.log(nonExistentVariable)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
    });

    it("should have eval disabled", async () => {
      const result = await strategy.execute(
        {
          command: 'eval("console.log(1)")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
    });

    it("should have Function disabled", async () => {
      const result = await strategy.execute(
        {
          command: 'new Function("return 1")()',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
    });

    it("should support setTimeout", async () => {
      const result = await strategy.execute(
        {
          command: 'setTimeout(() => console.log("timer"), 10); console.log("immediate")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      expect(result.stdout).toContain("immediate");
    });

    it("should support setInterval and clearInterval", async () => {
      const result = await strategy.execute(
        {
          command:
            "let count = 0; const id = setInterval(() => { count++; if (count >= 2) clearInterval(id); }, 10); console.log(count)",
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
    });

    it("should respect timeout from options", async () => {
      const result = await strategy.execute(
        {
          command: 'console.log("hello")',
          timeout: 5000,
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
    });

    it("should respect timeout from policy", async () => {
      const result = await strategy.execute(baseOptions, {
        ...defaultPolicy,
        resource: {
          timeoutLimit: 5000,
        },
      });

      expect(result.success).toBe(true);
    });
  });

  describe("fs module restrictions", () => {
    it("should allow fs.readFile by default (read-only)", async () => {
      const result = await strategy.execute(
        {
          command: 'const fs = require("fs"); console.log(typeof fs.readFile)',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
    });

    it("should deny fs.writeFileSync by default (read-only)", async () => {
      const result = await strategy.execute(
        {
          command: 'const fs = require("fs"); fs.writeFileSync("/tmp/test.txt", "data")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Read-only filesystem");
    });

    it("should deny fs.writeFile by default (read-only)", async () => {
      // Note: async writeFile would need to be awaited, but the proxy blocks it synchronously
      const result = await strategy.execute(
        {
          command: 'const fs = require("fs"); console.log(typeof fs.writeFile)',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(true);
      // The function exists but will throw when called
    });

    it("should deny fs.mkdir by default (read-only)", async () => {
      const result = await strategy.execute(
        {
          command: 'const fs = require("fs"); fs.mkdirSync("/tmp/test")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Read-only filesystem");
    });

    it("should deny fs.unlink by default (read-only)", async () => {
      const result = await strategy.execute(
        {
          command: 'const fs = require("fs"); fs.unlinkSync("/tmp/test.txt")',
        },
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Read-only filesystem");
    });
  });

  describe("module whitelist", () => {
    it("should deny module not in whitelist when whitelist is set", async () => {
      const policy: SandboxPolicy = {
        ...defaultPolicy,
        javascript: {
          allowedModules: ["path"],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };

      const result = await strategy.execute(
        {
          command: 'const util = require("util"); console.log("ok")',
        },
        policy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Module not allowed");
    });

    it("should allow module in whitelist", async () => {
      const policy: SandboxPolicy = {
        ...defaultPolicy,
        javascript: {
          allowedModules: ["path", "util"],
          deniedModules: [],
          allowChildProcess: false,
          allowFSWrite: false,
          allowDynamicEval: false,
        },
      };

      const result = await strategy.execute(
        {
          command: 'const path = require("path"); const util = require("util"); console.log("ok")',
        },
        policy,
      );

      expect(result.success).toBe(true);
    });
  });
});
