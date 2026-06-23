/**
 * Python AST Analyzer Strategy
 *
 * Performs AST-level static analysis on Python code before execution.
 * Detects dangerous imports, attribute calls, and builtin usage.
 * Falls back to PythonBuiltinHookStrategy for actual execution after analysis passes.
 *
 * Architecture reference: docs/infra/sandbox/strategies/python-sandbox.md
 */

import type {
  SandboxPolicy,
  PythonPolicy,
  ScriptExecutionResult,
  StrategyExecuteOptions,
} from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";
import { PythonBuiltinHookStrategy } from "./builtin-hook.js";
import { checkPythonAvailable, DEFAULT_DENIED_MODULES } from "./base.js";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { spawnSync } from "node:child_process";

/**
 * AST Analysis result from the subprocess.
 */
interface ASTAnalysisResult {
  safe: boolean;
  violations: string[];
}

/**
 * Python AST Analyzer Strategy
 *
 * Runs Python's `ast` module as a subprocess to analyze code before execution.
 * Priority is higher than builtin-hook (25 > 20), so it is preferred when available.
 *
 * Accepts an optional PythonBuiltinHookStrategy instance for dependency injection,
 * enabling the resolver to manage the delegation chain explicitly.
 */
export class PythonASTAnalyzerStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "ast-analyzer";
  name = "Python AST Analyzer";
  description = "AST-level static analysis for Python code with import and call validation";
  priority = 25;

  private builtinHook: PythonBuiltinHookStrategy;

  /**
   * @param builtinHook Optional PythonBuiltinHookStrategy instance for dependency injection.
   *                     When omitted, creates a new instance internally (fallback).
   */
  constructor(builtinHook?: PythonBuiltinHookStrategy) {
    this.builtinHook = builtinHook ?? new PythonBuiltinHookStrategy();
  }

  isAvailable(): boolean {
    return checkPythonAvailable();
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const code = options.command;
    const pyPolicy: PythonPolicy = {
      allowedModules: policy.python?.allowedModules ?? [],
      deniedModules: policy.python?.deniedModules ?? DEFAULT_DENIED_MODULES,
      allowSubprocess: policy.python?.allowSubprocess ?? false,
      restrictBuiltinOpen: policy.python?.restrictBuiltinOpen ?? true,
      allowDynamicEval: policy.python?.allowDynamicEval ?? false,
    };

    if (!code) {
      return {
        success: false,
        scriptName: "sandbox-python",
        executionTime: Date.now() - startTime,
        error: "Empty Python code",
      };
    }

    // Run AST analysis
    const analysis = await this.analyzeAST(code, pyPolicy);
    if (!analysis.safe) {
      return {
        success: false,
        scriptName: "sandbox-python",
        executionTime: Date.now() - startTime,
        error: `Security violation: ${analysis.violations.join(", ")}`,
        stderr: `AST analysis violations:\n  - ${analysis.violations.join("\n  - ")}`,
      };
    }

    // Analysis passed, delegate to builtin-hook for execution
    return this.builtinHook.execute(options, policy);
  }

  /**
   * Run AST analysis on the given Python code via a subprocess.
   */
  private async analyzeAST(code: string, policy: PythonPolicy): Promise<ASTAnalysisResult> {
    const deniedModulesJson = JSON.stringify(policy.deniedModules);
    const allowedModulesJson = JSON.stringify(policy.allowedModules);
    const allowSubprocess = policy.allowSubprocess ? "True" : "False";

    const analyzerScript = `
import ast, json, sys

code = sys.stdin.read()
DENIED_MODULES = set(${deniedModulesJson})
ALLOWED_MODULES = set(${allowedModulesJson})
ALLOW_SUBPROCESS = ${allowSubprocess}
DENIED_BUILTINS = {"eval", "exec", "compile"}
DENIED_FUNCTIONS = {"remove", "unlink", "rmdir", "chmod", "chown", "kill"}

violations = []

try:
    tree = ast.parse(code)
except SyntaxError as e:
    print(json.dumps({"safe": False, "violations": [f"Syntax error: {e}"]}))
    sys.exit(0)

for node in ast.walk(tree):
    if isinstance(node, ast.Import):
        for alias in node.names:
            name = alias.name.split(".")[0]
            if ALLOWED_MODULES and name not in ALLOWED_MODULES:
                violations.append(f"Module not allowed: {alias.name}")
            elif name in DENIED_MODULES and name != "subprocess":
                violations.append(f"Module denied: {alias.name}")
            elif name == "subprocess" and not ALLOW_SUBPROCESS:
                violations.append(f"Module denied: subprocess")

    if isinstance(node, ast.ImportFrom):
        if node.module:
            base = node.module.split(".")[0]
            if ALLOWED_MODULES and base not in ALLOWED_MODULES:
                violations.append(f"Module not allowed: {node.module}")
            elif base in DENIED_MODULES and base != "subprocess":
                violations.append(f"Module denied: {node.module}")
            elif base == "subprocess" and not ALLOW_SUBPROCESS:
                violations.append(f"Module denied: subprocess")

    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute):
        if isinstance(node.func.value, ast.Name):
            if node.func.attr in DENIED_FUNCTIONS:
                violations.append(
                    f"Dangerous function call: {node.func.value.id}.{node.func.attr}"
                )

    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
        if node.func.id in DENIED_BUILTINS:
            violations.append(f"Dangerous builtin call: {node.func.id}()")
        if node.func.id == "open":
            for arg in node.args:
                if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                    pass
            for kw in node.keywords:
                if kw.arg == "mode" and isinstance(kw.value, ast.Constant):
                    mode = str(kw.value.value)
                    if "w" in mode or "a" in mode or "x" in mode or "+" in mode:
                        violations.append("Write mode in open() call")
                if kw.arg == "mode" and isinstance(kw.value, ast.Str):
                    mode = kw.value.s
                    if "w" in mode or "a" in mode or "x" in mode or "+" in mode:
                        violations.append("Write mode in open() call")

print(json.dumps({"safe": len(violations) == 0, "violations": violations}))
`;

    const tmpFile = path.join(
      os.tmpdir(),
      `sandbox-ast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}.py`,
    );

    try {
      fs.writeFileSync(tmpFile, analyzerScript, "utf-8");

      const result = spawnSync("python", [tmpFile], {
        input: code,
        timeout: 10000,
        stdio: ["pipe", "pipe", "pipe"],
        encoding: "utf-8",
        env: {
          PYTHONPATH: "",
          PYTHONDONTWRITEBYTECODE: "1",
        },
      });

      if (result.status !== 0 || !result.stdout) {
        return {
          safe: false,
          violations: [`AST analyzer subprocess failed: ${result.stderr || "Unknown error"}`],
        };
      }

      return JSON.parse(result.stdout.trim()) as ASTAnalysisResult;
    } catch (error) {
      return {
        safe: false,
        violations: [
          `AST analysis error: ${error instanceof Error ? error.message : String(error)}`,
        ],
      };
    } finally {
      try {
        fs.unlinkSync(tmpFile);
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}
