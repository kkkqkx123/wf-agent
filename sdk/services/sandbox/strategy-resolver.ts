/**
 * Default Strategy Resolver
 *
 * Resolves strategy identifiers to concrete implementations with priority-based fallback.
 * Architecture reference: docs/infra/sandbox/architecture.md §4.3
 */

import type {
  SandboxPolicy,
  ScriptExecutionResult,
  StrategyExecuteOptions,
  StrategyImplementation,
  StrategyResolver,
  ShellSandboxStrategy,
  PythonSandboxStrategy,
  JavaScriptSandboxStrategy,
  LuaSandboxStrategy,
} from "@wf-agent/types";

import { ShellStaticAnalyzerStrategy } from "./strategies/shell-static-analyzer.js";
import {
  PythonBuiltinHookStrategy,
  PythonASTAnalyzerStrategy,
} from "./strategies/python-strategies/index.js";
import { JavaScriptVmContextStrategy } from "./strategies/js-vm-context.js";
import {
  LuaBuiltinHookStrategy,
  LuaStaticAnalyzerStrategy,
} from "./strategies/lua-strategies/index.js";
import {
  LinuxSeccompStrategy,
  WindowsJobObjectStrategy,
  ProotLikeRedirectStrategy,
} from "./strategies/os-hooks/index.js";
import { getTerminalService } from "../terminal/index.js";

class PassthroughStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "passthrough";
  name = "Passthrough (no sandbox)";
  description = "Fallback strategy that executes commands without sandbox restrictions";
  priority = 0;

  isAvailable(): boolean {
    return true;
  }

  async execute(
    options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    try {
      const terminalService = getTerminalService();
      const result = await terminalService.executeOneOff(options.command, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });
      return {
        success: result.success,
        scriptName: "sandbox-passthrough",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-passthrough",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}

type Language = "shell" | "python" | "javascript" | "lua";

export class DefaultStrategyResolver implements StrategyResolver {
  private shellStrategies = new Map<string, StrategyImplementation<unknown>>();
  private pythonStrategies = new Map<string, StrategyImplementation<unknown>>();
  private javascriptStrategies = new Map<string, StrategyImplementation<unknown>>();
  private luaStrategies = new Map<string, StrategyImplementation<unknown>>();

  constructor() {
    this.registerDefaultStrategies();
  }

  resolveShellStrategy(id: ShellSandboxStrategy | string): StrategyImplementation<unknown> {
    return this.shellStrategies.get(id) ?? new PassthroughStrategy();
  }

  resolvePythonStrategy(id: PythonSandboxStrategy | string): StrategyImplementation<unknown> {
    return this.pythonStrategies.get(id) ?? new PassthroughStrategy();
  }

  resolveJavaScriptStrategy(
    id: JavaScriptSandboxStrategy | string,
  ): StrategyImplementation<unknown> {
    return this.javascriptStrategies.get(id) ?? new PassthroughStrategy();
  }

  resolveLuaStrategy(id: LuaSandboxStrategy | string): StrategyImplementation<unknown> {
    return this.luaStrategies.get(id) ?? new PassthroughStrategy();
  }

  registerStrategy(language: Language, impl: StrategyImplementation<unknown>): void {
    const registry = this.getRegistry(language);
    registry.set(impl.id, impl);
  }

  resolveBest(language: Language, preferredIds: string[]): StrategyImplementation<unknown> {
    const registry = this.getRegistry(language);

    for (const id of preferredIds) {
      const strategy = registry.get(id);
      if (strategy && strategy.isAvailable()) {
        return strategy;
      }
    }

    const sorted = Array.from(registry.values())
      .filter(s => s.isAvailable())
      .sort((a, b) => b.priority - a.priority);

    return sorted[0] ?? new PassthroughStrategy();
  }

  private registerDefaultStrategies(): void {
    this.shellStrategies.set("static-analyzer", new ShellStaticAnalyzerStrategy());

    // Register OS Hook strategies — each has a unique platform-specific id.
    // Resolver uses id directly; SandboxRuntime maps "os-hook" to the
    // platform-specific id before calling resolveBest().
    const terminalService = getTerminalService();
    this.shellStrategies.set("linux-seccomp", new LinuxSeccompStrategy(terminalService));
    this.shellStrategies.set("windows-job", new WindowsJobObjectStrategy(terminalService));
    this.shellStrategies.set("proot-redirect", new ProotLikeRedirectStrategy(terminalService));

    // Register Python strategies — inject builtin-hook into ast-analyzer
    // to decouple the delegation chain from direct instantiation.
    const builtinHook = new PythonBuiltinHookStrategy();
    this.pythonStrategies.set("builtin-hook", builtinHook);
    this.pythonStrategies.set("ast-analyzer", new PythonASTAnalyzerStrategy(builtinHook));

    this.javascriptStrategies.set("vm-context", new JavaScriptVmContextStrategy());

    // Register Lua strategies — inject builtin-hook into static-analyzer
    // to decouple the delegation chain from direct instantiation.
    const luaBuiltinHook = new LuaBuiltinHookStrategy();
    this.luaStrategies.set("builtin-hook", luaBuiltinHook);
    this.luaStrategies.set("static-analyzer", new LuaStaticAnalyzerStrategy(luaBuiltinHook));
  }

  private getRegistry(language: Language): Map<string, StrategyImplementation<unknown>> {
    switch (language) {
      case "shell":
        return this.shellStrategies;
      case "python":
        return this.pythonStrategies;
      case "javascript":
        return this.javascriptStrategies;
      case "lua":
        return this.luaStrategies;
    }
  }
}
