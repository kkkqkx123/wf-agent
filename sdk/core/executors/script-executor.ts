/**
 * Script Executor using Terminal Service
 * 
 * Simplified script execution that leverages the Terminal Service.
 * Scripts are treated as shell commands without type distinctions.
 */

import type { Script, ScriptExecutionOptions, ScriptExecutionResult } from "@wf-agent/types";
import { getTerminalService } from "../../services/terminal/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptExecutor" });

/**
 * Script Executor
 * 
 * Executes scripts using the Terminal Service's executeOneOff method.
 * All scripts are treated as shell commands - no type distinctions needed.
 */
export class ScriptExecutor {
  private terminalService = getTerminalService();

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
    
    // Get command content
    const command = script.content || '';
    
    if (!command) {
      const executionTime = Date.now() - startTime;
      return {
        success: false,
        scriptName: script.name,
        executionTime,
        error: 'Script content is empty',
      };
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
    // Terminal service handles its own cleanup
    logger.debug("Script executor cleanup completed");
  }
}
