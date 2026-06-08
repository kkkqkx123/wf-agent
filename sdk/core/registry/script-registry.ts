/**
 * Script Registry
 * Provides a unified interface for script management and execution
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 *
 */

import type {
  Script,
  ScriptExecutionOptions,
  ScriptExecutionResult,
  ScriptFlow,
} from "@wf-agent/types";
import { ScriptExecutor } from '../executors/script-executor.js';
import { ScriptEngine } from '../script/engine/script-engine.js';
import { ScriptFlowEngine } from '../script/engine/script-flow-engine.js';
import {
  ScriptExecutionError,
  ScriptNotFoundError,
  ConfigurationValidationError,
} from "@wf-agent/types";
import { all, ok, err, getErrorMessage } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { ScriptStorageAdapter } from "@wf-agent/storage";
import {
  persistScript,
  removeScript,
  initializeScriptsFromStorage,
} from "./utils/script-storage-utils.js";

const logger = createContextualLogger({ component: "ScriptRegistry" });

/**
 * Script Registry Class
 * Integrates script registry and executor management functions
 */
class ScriptRegistry {
  private scripts: Map<string, Script> = new Map();
  private flows: Map<string, ScriptFlow> = new Map();
  private executor: ScriptExecutor;
  private scriptEngine: ScriptEngine | null = null;
  private flowEngine: ScriptFlowEngine | null = null;

  constructor(
    executor?: ScriptExecutor,
    private readonly storageAdapter: ScriptStorageAdapter | null = null,
  ) {
    this.executor = executor ?? new ScriptExecutor();
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
    logger.info("Script registered", { scriptName: script.name });

    // Persist to storage (async, non-blocking)
    if (this.storageAdapter) {
      persistScript(scriptWithDefaults, this.storageAdapter).catch(error => {
        logger.error("Failed to persist script during registration", {
          scriptName: script.name,
          error: getErrorMessage(error),
        });
      });
    }
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

    // Remove from storage (async, non-blocking)
    if (this.storageAdapter) {
      removeScript(scriptName, this.storageAdapter).catch(error => {
        logger.error("Failed to remove script from storage", {
          scriptName,
          error: getErrorMessage(error),
        });
      });
    }
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

    // Persist to storage (async, non-blocking)
    if (this.storageAdapter) {
      persistScript(updatedScript, this.storageAdapter).catch(error => {
        logger.error("Failed to persist script update", {
          scriptName,
          error: getErrorMessage(error),
        });
      });
    }
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

    if (!script.description || typeof script.description !== "string") {
      throw new ConfigurationValidationError(
        "Script description is required and must be a string",
        {
          configType: "script",
          field: "description",
        },
      );
    }

    // Verify that the script content or file path or template contains at least one of the following:
    if (!script.content && !script.filePath && !script.template) {
      throw new ConfigurationValidationError("Script must have either content, filePath, or template", {
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

    // Get script definition
    const script = this.getScript(scriptName);

    // Execute using the simplified ScriptExecutor
    const result = await this.executor.execute(script, options);

    if (!result.success) {
      return err(
        new ScriptExecutionError(
          result.error || 'Script execution failed',
          scriptName,
          { options },
        ),
      );
    }

    logger.debug("Script execution completed", { scriptName, success: result.success });
    return ok(result);
  }

  /**
   * Execute the script with ScriptEngine (supports template + executor mode)
   * @param scriptName The name of the script
   * @param options Execution options
   * @param args Runtime argument values for template rendering
   * @returns Execution result
   */
  async executeWithEngine(
    scriptName: string,
    options: Partial<ScriptExecutionOptions> = {},
    args: Record<string, unknown> = {},
  ): Promise<Result<ScriptExecutionResult, ScriptExecutionError>> {
    const script = this.getScript(scriptName);

    if (!this.scriptEngine) {
      this.scriptEngine = new ScriptEngine();
    }

    const result = await this.scriptEngine.execute(script, options, { args });

    if (!result.success) {
      return err(
        new ScriptExecutionError(
          result.error || 'Script execution failed',
          scriptName,
          { options, args },
        ),
      );
    }

    return ok(result);
  }

  /**
   * Register a flow blueprint
   * @param flow Flow blueprint definition
   * @throws ConfigurationValidationError If the flow name already exists
   */
  registerFlow(flow: ScriptFlow): void {
    if (this.flows.has(flow.name)) {
      logger.warn("Flow already exists", { flowName: flow.name });
      throw new ConfigurationValidationError(`Flow with name '${flow.name}' already exists`, {
        field: "name",
      });
    }
    this.flows.set(flow.name, flow);
    logger.info("Flow registered", { flowName: flow.name });
  }

  /**
   * Get a flow blueprint
   * @param flowName Flow name
   * @returns Flow blueprint
   */
  getFlow(flowName: string): ScriptFlow {
    const flow = this.flows.get(flowName);
    if (!flow) {
      throw new Error(`Flow '${flowName}' not found`);
    }
    return flow;
  }

  /**
   * List all registered flows
   * @returns Array of flow blueprints
   */
  listFlows(): ScriptFlow[] {
    return Array.from(this.flows.values());
  }

  /**
   * Execute a flow blueprint
   * @param flowName Flow name
   * @returns Flow execution result
   */
  async executeFlow(flowName: string): Promise<import('../script/engine/script-flow-engine.js').FlowExecutionResult> {
    const flow = this.getFlow(flowName);

    if (!this.scriptEngine) {
      this.scriptEngine = new ScriptEngine();
    }
    if (!this.flowEngine) {
      this.flowEngine = new ScriptFlowEngine(this.scriptEngine, this.scripts);
    }

    return this.flowEngine.execute(flow);
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

  // ============================================================
  // Storage Initialization
  // ============================================================

  /**
   * Initialize scripts from storage
   * Loads all persisted script definitions into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeScriptsFromStorage(this.storageAdapter, this.scripts);
  }
}

/**
 * Export the ScriptRegistry class
 */
export { ScriptRegistry };
