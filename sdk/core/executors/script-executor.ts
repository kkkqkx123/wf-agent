/**
 * Script Executor using Terminal Service
 * 
 * Simplified script execution that leverages the Terminal Service.
 * Scripts are treated as shell commands without type distinctions.
 * Supports template rendering and executor mode selection.
 */

import type { Script, ScriptExecutionOptions, ScriptExecutionResult, ExecutorMode } from "@wf-agent/types";
import { getTerminalService, type TerminalService } from "../../services/terminal/index.js";
import { ScriptTemplateEngine } from "../script/engine/script-template.js";
import { DirectExecutor } from "../script/executors/direct-executor.js";
import { SharedExecutor } from "../script/executors/shared-executor.js";
import type { BaseExecutor } from "../script/executors/base-executor.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptExecutor" });

/**
 * Script Executor
 * 
 * Executes scripts using the Terminal Service's executeOneOff method.
 * All scripts are treated as shell commands - no type distinctions needed.
 */
export class ScriptExecutor {
  private terminalService: TerminalService;
  private templateEngine: ScriptTemplateEngine;
  private executors: Map<ExecutorMode, BaseExecutor>;

  constructor(terminalService?: TerminalService) {
    // Allow injection for testing, otherwise use default
    this.terminalService = terminalService || getTerminalService();
    this.templateEngine = new ScriptTemplateEngine();
    this.executors = new Map();
    this.executors.set("direct", new DirectExecutor(this.terminalService));
    this.executors.set("shared", new SharedExecutor(this.terminalService));
  }

  /**
   * Execute a script
   * @param script Script definition
   * @param options Execution options
   * @returns Execution result
   */
  async execute(
    script: Script,
    options?: ScriptExecutionOptions
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    let command: string;

    if (script.template) {
      const args = script.arguments || [];
      const resolvedArgs = this.templateEngine.resolveArguments(args, options?.environment || {});
      const renderResult = this.templateEngine.render(script.template, resolvedArgs as Record<string, unknown>);
      command = renderResult.command;
    } else {
      command = script.content || '';
    }

    if (!command) {
      const executionTime = Date.now() - startTime;
      return {
        success: false,
        scriptName: script.name,
        executionTime,
        error: 'Script content is empty',
      };
    }

    const executorMode: ExecutorMode = script.executor?.mode || options?.executorMode || "direct";
    const executor = this.executors.get(executorMode);

    if (executor) {
      try {
        const result = await executor.execute({
          command,
          cwd: script.executor?.cwd || options?.workingDirectory,
          env: { ...script.executor?.environment, ...options?.environment },
          timeout: options?.timeout,
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

    try {
      // Execute using terminal service (stateless execution)
      const result = await this.terminalService.executeOneOff(command, {
        cwd: options?.workingDirectory,
        env: options?.environment,
        timeout: options?.timeout,
      });

      const executionTime = Date.now() - startTime;

      logger.debug("Script execution completed", {
        scriptName: script.name,
        success: result.success,
        exitCode: result.exitCode,
        executionTime,
      });

      return {
        success: result.success,
        scriptName: script.name,
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime,
        error: result.error,
      };
    } catch (error) {
      const executionTime = Date.now() - startTime;
      
      logger.error("Script execution failed", {
        scriptName: script.name,
        error: error instanceof Error ? error.message : String(error),
      });

      return {
        success: false,
        scriptName: script.name,
        executionTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    for (const executor of this.executors.values()) {
      await executor.cleanup();
    }
    logger.debug("Script executor cleanup completed");
  }
}
