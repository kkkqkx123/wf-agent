/**
 * Script Engine
 * Core orchestration engine: template rendering -> argument resolution -> execution
 */

import type {
  Script,
  ScriptExecutionOptions,
  ScriptExecutionResult,
  ExecutorMode,
} from "@wf-agent/types";
import { ScriptTemplateEngine } from "./script-template.js";
import { ArgumentResolver } from "../resolvers/argument-resolver.js";
import { DynamicResolver } from "../resolvers/dynamic-resolver.js";
import { DirectExecutor } from "../executors/direct-executor.js";
import { SharedExecutor } from "../executors/shared-executor.js";
import { PtyExecutor } from "../executors/pty-executor.js";
import { SandboxShellExecutor } from "../executors/sandbox-shell-executor.js";
import { SandboxPythonExecutor } from "../executors/sandbox-python-executor.js";
import { SandboxJavaScriptExecutor } from "../executors/sandbox-javascript-executor.js";
import type { BaseExecutor } from "../executors/base-executor.js";
import { getTerminalService } from "../../../services/terminal/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptEngine" });

/**
 * Script Engine Options
 */
export interface ScriptEngineOptions {
  /** Runtime argument values */
  args?: Record<string, unknown>;
  /** Workflow context variables for dynamic resolution */
  contextVariables?: Record<string, unknown>;
}

/**
 * Script Engine
 * Orchestrates the full script execution pipeline
 */
export class ScriptEngine {
  private templateEngine: ScriptTemplateEngine;
  private argumentResolver: ArgumentResolver;
  private dynamicResolver: DynamicResolver;
  private executors: Map<ExecutorMode, BaseExecutor>;

  constructor() {
    this.templateEngine = new ScriptTemplateEngine();
    this.argumentResolver = new ArgumentResolver();
    this.dynamicResolver = new DynamicResolver();

    const terminalService = getTerminalService();
    this.executors = new Map();
    this.executors.set("direct", new DirectExecutor(terminalService));
    this.executors.set("shared", new SharedExecutor(terminalService));
    this.executors.set("pty", new PtyExecutor());
    this.executors.set("sandbox-shell", new SandboxShellExecutor(terminalService));
    this.executors.set("sandbox-python", new SandboxPythonExecutor(terminalService));
    this.executors.set("sandbox-javascript", new SandboxJavaScriptExecutor(terminalService));
  }

  /**
   * Execute a script with the engine
   * @param script Script definition (must have template or content)
   * @param options Execution options
   * @param engineOptions Engine-specific options (args, context)
   * @returns Execution result
   */
  async execute(
    script: Script,
    options?: ScriptExecutionOptions,
    engineOptions?: ScriptEngineOptions,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    let command: string;

    if (script.template) {
      const args = script.arguments || [];
      const providedArgs = engineOptions?.args || {};
      const contextVars = engineOptions?.contextVariables || {};

      const resolvedArgs = this.argumentResolver.resolve(args, providedArgs, contextVars);
      const dynamicArgs = this.dynamicResolver.resolve(resolvedArgs, contextVars);

      const renderResult = this.templateEngine.render(script.template, dynamicArgs as Record<string, unknown>);

      if (!renderResult.resolved) {
        logger.warn("Template has unresolved placeholders", {
          scriptName: script.name,
          placeholders: renderResult.unresolvedPlaceholders,
        });
      }

      command = renderResult.command;
    } else {
      command = script.content || "";
    }

    if (!command) {
      return {
        success: false,
        scriptName: script.name,
        executionTime: Date.now() - startTime,
        error: "No command to execute (empty template or content)",
      };
    }

    const executorMode: ExecutorMode = script.executor?.mode || options?.executorMode || "direct";
    const executor = this.executors.get(executorMode);

    if (!executor) {
      return {
        success: false,
        scriptName: script.name,
        executionTime: Date.now() - startTime,
        error: `Unknown executor mode: ${executorMode}`,
      };
    }

    try {
      const result = await executor.execute({
        command,
        cwd: script.executor?.cwd || options?.workingDirectory,
        env: { ...script.executor?.environment, ...options?.environment },
        timeout: options?.timeout,
        sandboxConfig: options?.sandboxConfig ?? script.options?.sandboxConfig,
        language: options?.language ?? script.language,
      });

      return {
        ...result,
        scriptName: script.name,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: script.name,
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Cleanup all executors
   */
  async cleanup(): Promise<void> {
    for (const [mode, executor] of this.executors) {
      await executor.cleanup();
      logger.debug("Executor cleaned up", { mode });
    }
  }
}