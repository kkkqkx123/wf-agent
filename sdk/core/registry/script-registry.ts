/**
 * Script Registry
 * Provides a unified interface for script management and execution
 *
 * This module only exports class definitions; instances are not exported. Instances are managed uniformly through a Dependency Injection (DI) container.
 *
 */

import type {
  Script,
  ScriptType,
  ScriptExecutionOptions,
  ScriptExecutionResult,
} from "@wf-agent/types";
import type { IScriptExecutor } from "@wf-agent/script-executors";
import {
  ShellExecutor,
  PythonExecutor,
  JavaScriptExecutor,
  PowerShellExecutor,
  CmdExecutor,
} from "@wf-agent/script-executors";
import {
  ScriptExecutionError,
  ScriptNotFoundError,
  ConfigurationValidationError,
} from "@wf-agent/types";
import { tryCatchAsyncWithSignal, all } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptRegistry" });

/**
 * Script Registry Class
 * Integrates script registry and executor management functions
 */
class ScriptRegistry {
  private scripts: Map<string, Script> = new Map();
  private executors: Map<ScriptType, IScriptExecutor> = new Map();

  constructor() {
    this.initializeExecutors();
  }

  /**
   * Initialize the executor
   * The script type is the standard type of SDK, using static initialization.
   */
  private initializeExecutors(): void {
    this.executors.set("SHELL", new ShellExecutor());
    this.executors.set("PYTHON", new PythonExecutor());
    this.executors.set("JAVASCRIPT", new JavaScriptExecutor());
    this.executors.set("POWERSHELL", new PowerShellExecutor());
    this.executors.set("CMD", new CmdExecutor());
  }

  /**
   * Registration Script
   * @param script Script definition
   * @throws ValidationError If the script definition is invalid or the name already exists
   */
  registerScript(script: Script): void {
    // Verify script definitions
    this.validateScript(script);

    // Set default values
    const scriptWithDefaults: Script = {
      ...script,
      enabled: script.enabled !== undefined ? script.enabled : true,
    };

    // Check if the script name already exists.
    if (this.scripts.has(script.name)) {
      logger.warn("Script already exists", { scriptName: script.name });
      throw new ConfigurationValidationError(`Script with name '${script.name}' already exists`, {
        configType: "script",
        field: "name",
      });
    }

    // Registration script
    this.scripts.set(script.name, scriptWithDefaults);
    logger.info("Script registered", { scriptName: script.name, scriptType: script.type });
  }

  /**
   * Batch registration script
   * @param scripts: An array of script definitions
   */
  registerScripts(scripts: Script[]): void {
    for (const script of scripts) {
      this.registerScript(script);
    }
  }

  /**
   * Script Deletion
   * @param scriptName The name of the script
   * @throws NotFoundError If the script does not exist
   */
  unregisterScript(scriptName: string): void {
    if (!this.scripts.has(scriptName)) {
      logger.warn("Attempted to unregister non-existent script", { scriptName });
      throw new ScriptNotFoundError(`Script '${scriptName}' not found`, scriptName);
    }
    this.scripts.delete(scriptName);
    logger.info("Script unregistered", { scriptName });
  }

  /**
   * Get script definition
   * @param scriptName Script name
   * @returns Script definition
   * @throws NotFoundError If the script does not exist
   */
  getScript(scriptName: string): Script {
    const script = this.scripts.get(scriptName);
    if (!script) {
      throw new ScriptNotFoundError(`Script '${scriptName}' not found`, scriptName);
    }
    return script;
  }

  /**
   * Get the script definition (may return undefined)
   * @param scriptName The name of the script
   * @returns The script definition; returns undefined if it does not exist
   */
  findScript(scriptName: string): Script | undefined {
    return this.scripts.get(scriptName);
  }

  /**
   * List all scripts
   * @returns Array of script definitions
   */
  listScripts(): Script[] {
    return Array.from(this.scripts.values());
  }

  /**
   * List scripts by type
   * @param type: Script type
   * @returns: Array of script definitions
   */
  listScriptsByType(type: string): Script[] {
    return this.listScripts().filter(script => script.type === type);
  }

  /**
   * List scripts by category
   * @param category Script category
   * @returns Array of script definitions
   */
  listScriptsByCategory(category: string): Script[] {
    return this.listScripts().filter(script => script.metadata?.category === category);
  }

  /**
   * Search Script
   * @param query Search keyword
   * @returns Array of matching scripts
   */
  searchScripts(query: string): Script[] {
    const lowerQuery = query.toLowerCase();
    return this.listScripts().filter(script => {
      return (
        script.name.toLowerCase().includes(lowerQuery) ||
        script.description.toLowerCase().includes(lowerQuery) ||
        script.metadata?.tags?.some(tag => tag.toLowerCase().includes(lowerQuery)) ||
        script.metadata?.category?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * Check if the script exists
   * @param scriptName The name of the script
   * @returns Whether it exists or not
   */
  hasScript(scriptName: string): boolean {
    return this.scripts.has(scriptName);
  }

  /**
   * Clear all scripts
   */
  clearScripts(): void {
    const count = this.scripts.size;
    this.scripts.clear();
    logger.info("All scripts cleared", { count });
  }

  /**
   * Get the number of scripts
   * @returns The number of scripts
   */
  scriptCount(): number {
    return this.scripts.size;
  }

  /**
   * Update script definition
   * @param scriptName Script name
   * @param updates Update content
   * @throws NotFoundError If the script does not exist
   */
  updateScript(scriptName: string, updates: Partial<Script>): void {
    const script = this.getScript(scriptName);

    const updatedScript = {
      ...script,
      ...updates,
      // Make sure the 'enabled' field has a default value.
      enabled: updates.enabled !== undefined ? updates.enabled : (script.enabled ?? true),
    };

    this.validateScript(updatedScript);
    this.scripts.set(scriptName, updatedScript);
  }

  /**
   * Enable the script
   * @param scriptName The name of the script
   * @throws NotFoundError If the script does not exist
   */
  enableScript(scriptName: string): void {
    this.updateScript(scriptName, { enabled: true });
  }

  /**
   * Disable the script
   * @param scriptName The name of the script
   * @throws NotFoundError If the script does not exist
   */
  disableScript(scriptName: string): void {
    this.updateScript(scriptName, { enabled: false });
  }

  /**
   * Check if the script is enabled.
   * @param scriptName: The name of the script
   * @returns: Whether it is enabled or not
   * @throws: NotFoundError: If the script does not exist
   */
  isScriptEnabled(scriptName: string): boolean {
    const script = this.getScript(scriptName);
    return script.enabled ?? true;
  }

  /**
   * Verify script definition
   * @param script The script definition
   * @returns Whether it is valid
   * @throws ValidationError If the script definition is invalid
   */
  validateScript(script: Script): boolean {
    // Verify required fields
    if (!script.name || typeof script.name !== "string") {
      throw new ConfigurationValidationError("Script name is required and must be a string", {
        configType: "script",
        field: "name",
      });
    }

    if (!script.type || typeof script.type !== "string") {
      throw new ConfigurationValidationError("Script type is required and must be a string", {
        configType: "script",
        field: "type",
      });
    }

    if (!script.description || typeof script.description !== "string") {
      throw new ConfigurationValidationError(
        "Script description is required and must be a string",
        {
          configType: "script",
          field: "description",
        },
      );
    }

    // Verify that the script content or file path contains at least one of the following:
    if (!script.content && !script.filePath) {
      throw new ConfigurationValidationError("Script must have either content or filePath", {
        configType: "script",
        field: "content",
      });
    }

    // Verify execution options
    if (!script.options) {
      throw new ConfigurationValidationError("Script options are required", {
        configType: "script",
        field: "options",
      });
    }

    // Verify the timeout period
    if (script.options.timeout !== undefined && script.options.timeout < 0) {
      throw new ConfigurationValidationError("Script timeout must be a positive number", {
        configType: "script",
        field: "options.timeout",
      });
    }

    // Verify the number of retries.
    if (script.options.retries !== undefined && script.options.retries < 0) {
      throw new ConfigurationValidationError("Script retries must be a non-negative number", {
        configType: "script",
        field: "options.retries",
      });
    }

    // Verify retry delay
    if (script.options.retryDelay !== undefined && script.options.retryDelay < 0) {
      throw new ConfigurationValidationError("Script retryDelay must be a non-negative number", {
        configType: "script",
        field: "options.retryDelay",
      });
    }

    // Verify the `enabled` field (if provided).
    if (script.enabled !== undefined && typeof script.enabled !== "boolean") {
      throw new ConfigurationValidationError("Script enabled must be a boolean", {
        configType: "script",
        field: "enabled",
      });
    }

    return true;
  }

  /**
   * Execute the script
   * @param scriptName: The name of the script
   * @param options: Execution options that override the script's default settings
   * @returns: Result<ScriptExecutionResult, ScriptExecutionError>
   */
  async execute(
    scriptName: string,
    options: Partial<ScriptExecutionOptions> = {},
  ): Promise<Result<ScriptExecutionResult, ScriptExecutionError>> {
    logger.debug("Script execution started", { scriptName });

    // Obtain script definitions
    const script = this.getScript(scriptName);

    // Get the corresponding executor.
    const executor = this.executors.get(script.type);
    if (!executor) {
      return err(
        new ScriptExecutionError(
          `No executor found for script type '${script.type}'`,
          scriptName,
          script.type,
          { options },
        ),
      );
    }

    // Merge execution options (default script options + passed-in options)
    const executionOptions: ScriptExecutionOptions = {
      ...script.options,
      ...options,
    };

    // Use `tryCatchAsyncWithSignal` to ensure that the signal is passed correctly.
    const result = await tryCatchAsyncWithSignal<ScriptExecutionResult>(
      (signal: AbortSignal | undefined) =>
        executor.execute(script, { ...executionOptions, signal }),
      executionOptions?.signal,
    );

    if (result.isErr()) {
      return err(
        this.convertToScriptExecutionError(result.error, scriptName, script.type, executionOptions),
      );
    }

    logger.debug("Script execution completed", { scriptName, success: result.value.success });
    return ok(result.value);
  }

  /**
   * 批量执行脚本
   * @param executions 执行任务数组
   * @returns Result<ScriptExecutionResult[], ScriptExecutionError>
   */
  async executeBatch(
    executions: Array<{
      scriptName: string;
      options?: Partial<ScriptExecutionOptions>;
    }>,
  ): Promise<Result<ScriptExecutionResult[], ScriptExecutionError>> {
    // Execute all scripts in parallel.
    const results = await Promise.all(
      executions.map(exec => this.execute(exec.scriptName, exec.options)),
    );

    // Combine the results; return "success" if everything is successful, otherwise return the first error.
    return all(results);
  }

  /**
   * Translate from auto to en:
   *
   * Convert error to ScriptExecutionError
   *
   * @param error The original error
   * @param scriptName The script name
   * @param scriptType The script type
   */
  private convertToScriptExecutionError(
    error: unknown,
    scriptName: string,
    scriptType: string,
    options: ScriptExecutionOptions,
  ): ScriptExecutionError {
    // If it is already a ScriptExecutionError, return it directly.
    if (error instanceof ScriptExecutionError) {
      return error;
    }

    const message = error instanceof Error ? error.message : String(error);

    return new ScriptExecutionError(
      `Script execution failed: ${message}`,
      scriptName,
      scriptType,
      { options },
      error instanceof Error ? error : undefined,
    );
  }
}

/**
 * Export the ScriptRegistry class
 */
export { ScriptRegistry };
