/**
 * Lua Static Analyzer Strategy
 *
 * Performs static analysis on Lua code before execution.
 * Detects dangerous function calls, module usage, and patterns.
 * Falls back to LuaBuiltinHookStrategy for actual execution after analysis passes.
 *
 * Architecture reference: docs/infra/sandbox/strategies/lua-sandbox.md
 */

import type { SandboxPolicy, LuaPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";
import { LuaBuiltinHookStrategy } from "./builtin-hook.js";
import { checkLuaAvailable, DEFAULT_DENIED_MODULES } from "./base.js";

/**
 * Static analysis result.
 */
interface StaticAnalysisResult {
  safe: boolean;
  violations: string[];
}

/**
 * Lua Static Analyzer Strategy
 *
 * Performs pattern-based static analysis on Lua code before execution.
 * Priority is higher than builtin-hook (25 > 20), so it is preferred when available.
 *
 * Accepts an optional LuaBuiltinHookStrategy instance for dependency injection,
 * enabling the resolver to manage the delegation chain explicitly.
 */
export class LuaStaticAnalyzerStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "static-analyzer";
  name = "Lua Static Analyzer";
  description = "Static analysis for Lua code with function and module validation";
  priority = 25;

  private builtinHook: LuaBuiltinHookStrategy;

  /**
   * @param builtinHook Optional LuaBuiltinHookStrategy instance for dependency injection.
   *                     When omitted, creates a new instance internally (fallback).
   */
  constructor(builtinHook?: LuaBuiltinHookStrategy) {
    this.builtinHook = builtinHook ?? new LuaBuiltinHookStrategy();
  }

  isAvailable(): boolean {
    return checkLuaAvailable();
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const code = options.command;
    const luaPolicy: LuaPolicy = {
      allowedModules: policy.lua?.allowedModules ?? [],
      deniedModules: policy.lua?.deniedModules ?? DEFAULT_DENIED_MODULES,
      allowOsExecute: policy.lua?.allowOsExecute ?? false,
      restrictIoOpen: policy.lua?.restrictIoOpen ?? true,
      allowDynamicLoad: policy.lua?.allowDynamicLoad ?? false,
    };

    if (!code) {
      return {
        success: false,
        scriptName: "sandbox-lua",
        executionTime: Date.now() - startTime,
        error: "Empty Lua code",
      };
    }

    // Run static analysis
    const analysis = this.analyzeStatic(code, luaPolicy);
    if (!analysis.safe) {
      return {
        success: false,
        scriptName: "sandbox-lua",
        executionTime: Date.now() - startTime,
        error: `Security violation: ${analysis.violations.join(", ")}`,
        stderr: `Static analysis violations:\n  - ${analysis.violations.join("\n  - ")}`,
      };
    }

    // Analysis passed, delegate to builtin-hook for execution
    return this.builtinHook.execute(options, policy);
  }

  /**
   * Perform static analysis on Lua code using pattern matching.
   * This is a lightweight alternative to full AST parsing.
   */
  private analyzeStatic(code: string, policy: LuaPolicy): StaticAnalysisResult {
    const violations: string[] = [];

    // Normalize code for analysis
    const normalizedCode = code.replace(/--\[\[[\s\S]*?\]\]/g, "").replace(/--[^\n]*/g, "");

    // Check for dangerous function calls
    const dangerousFunctions = [
      { pattern: /\bos\.execute\s*\(/, name: "os.execute", deniedUnless: "allowOsExecute" },
      { pattern: /\bos\.remove\s*\(/, name: "os.remove" },
      { pattern: /\bos\.rename\s*\(/, name: "os.rename" },
      { pattern: /\bos\.exit\s*\(/, name: "os.exit" },
      { pattern: /\bio\.popen\s*\(/, name: "io.popen" },
      { pattern: /\bloadstring\s*\(/, name: "loadstring", deniedUnless: "allowDynamicLoad" },
      { pattern: /\bload\s*\(/, name: "load", deniedUnless: "allowDynamicLoad" },
      { pattern: /\bdofile\s*\(/, name: "dofile" },
      { pattern: /\bloadfile\s*\(/, name: "loadfile" },
    ];

    for (const func of dangerousFunctions) {
      if (func.pattern.test(normalizedCode)) {
        if (func.deniedUnless === "allowOsExecute" && policy.allowOsExecute) {
          continue;
        }
        if (func.deniedUnless === "allowDynamicLoad" && policy.allowDynamicLoad) {
          continue;
        }
        violations.push(`Dangerous function call: ${func.name}()`);
      }
    }

    // Check for require() calls with denied modules
    const requirePattern = /\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)/g;
    let requireMatch: RegExpExecArray | null;
    while ((requireMatch = requirePattern.exec(normalizedCode)) !== null) {
      const moduleName = requireMatch[1];
      if (!moduleName) continue;
      const moduleBase = moduleName.split(".")[0]!;
      if (policy.allowedModules.length > 0 && !policy.allowedModules.includes(moduleBase)) {
        violations.push(`Module not allowed: ${moduleName}`);
      }
      if (policy.deniedModules.includes(moduleBase)) {
        violations.push(`Module denied: ${moduleName}`);
      }
    }

    // Check for io.open with write mode
    if (policy.restrictIoOpen) {
      const ioOpenPattern = /\bio\.open\s*\([^,]+,\s*['"]([^'"]+)['"]\s*\)/g;
      let ioMatch: RegExpExecArray | null;
      while ((ioMatch = ioOpenPattern.exec(normalizedCode)) !== null) {
        const mode = ioMatch[1];
        if (!mode) continue;
        if (/[wax+]/.test(mode)) {
          violations.push(`Write mode in io.open(): ${mode}`);
        }
      }
    }

    // Check for global variable manipulation
    const globalPatterns = [
      { pattern: /_G\s*\[/, name: "_G index access" },
      { pattern: /_G\s*\./, name: "_G property access" },
      { pattern: /\bsetfenv\s*\(/, name: "setfenv" },
      { pattern: /\bgetfenv\s*\(/, name: "getfenv" },
    ];

    for (const gp of globalPatterns) {
      if (gp.pattern.test(normalizedCode)) {
        violations.push(`Dangerous pattern: ${gp.name}`);
      }
    }

    return {
      safe: violations.length === 0,
      violations,
    };
  }
}
