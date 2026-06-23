/**
 * Script Registry
 *
 * Manages script and flow blueprint registration, retrieval, and persistence.
 * Implements standardized registry interfaces for consistency.
 *
 * Responsibilities:
 * - Script CRUD (register, unregister, update, get, list, search)
 * - Flow blueprint management (register, get, list)
 * - Script validation
 * - Storage persistence (write-through)
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 *
 * Interface Implementation:
 * - Registry<Script>: Read operations
 * - MutableRegistry<Script>: Write operations
 * - BatchOperations<Script>: Batch register/unregister
 * - SearchableRegistry<Script>: Search and filter operations
 */

// Internal imports
import { createContextualLogger } from "../../utils/contextual-logger.js";
import {
  persistScript,
  removeScript,
  initializeScriptsFromStorage,
} from "./utils/storage/index.js";
import { createRegistry } from "./utils/index.js";
import type {
  Registry,
  MutableRegistry,
  BatchOperations,
  SearchableRegistry,
} from "./types.js";
import {
  RegistryNotFoundError,
  RegistryAlreadyExistsError,
  RegistryValidationError,
} from "./types.js";
import {
  validateRequiredString,
  validateAtLeastOne,
  validatePositiveNumber,
  validateBoolean,
} from "./utils/index.js";
import type { Result } from "@wf-agent/types";
import { ok, err, all } from "@wf-agent/common-utils";
import { ScriptExecutor as ScriptExecutor_ } from "../../services/executors/script-executor.js";
import { ScriptEngine } from "../../services/script/engine/script-engine.js";
import { ScriptFlowEngine } from "../../services/script/engine/script-flow-engine.js";

// External imports
import type {
  Script,
  ScriptExecutionOptions,
  ScriptExecutionResult,
  ScriptFlow,
} from "@wf-agent/types";
import {
  ScriptNotFoundError,
  ScriptExecutionError,
} from "@wf-agent/types";
import type { ScriptStorageAdapter } from "@wf-agent/storage";

const logger = createContextualLogger({ component: "ScriptRegistry" });

/**
 * Script Registry Class
 *
 * Implements:
 * - Registry<Script>: Read operations (get, has, list, keys, size, clear)
 * - MutableRegistry<Script>: Write operations (set, delete)
 * - BatchOperations<Script>: Batch register/unregister
 * - SearchableRegistry<Script>: Search and filter
 */
class ScriptRegistry
  implements
    Registry<Script>,
    MutableRegistry<Script>,
    BatchOperations<Script>,
    SearchableRegistry<Script>
{
  private scripts = createRegistry<Script>();
  private flows: Map<string, ScriptFlow> = new Map();

  constructor(private readonly storageAdapter: ScriptStorageAdapter | null = null) {}

  // ============================================================
  // Registry Interface Implementation (Read Operations)
  // ============================================================

  /** Get script by name, returns undefined if not found */
  get(key: string): Script | undefined {
    return this.scripts.get(key);
  }

  /** Check if script exists */
  has(key: string): boolean {
    return this.scripts.has(key);
  }

  /** List all scripts */
  list(): Script[] {
    return this.scripts.list();
  }

  /** Get all script names */
  keys(): string[] {
    return this.scripts.keys();
  }

  /** Get the number of scripts */
  get size(): number {
    return this.scripts.size;
  }

  /** Clear all scripts */
  clear(): void {
    const count = this.scripts.size;
    this.scripts.clear();
    logger.info("All scripts cleared", { count });
  }

  // ============================================================
  // MutableRegistry Interface Implementation (Write Operations)
  // ============================================================

  /** Set a script by name */
  set(key: string, value: Script): void {
    this.scripts.set(key, value);
  }

  /** Delete a script by name, returns true if deleted */
  delete(key: string): boolean {
    return this.scripts.delete(key);
  }

  // ============================================================
  // Core CRUD Operations (Standardized Naming)
  // ============================================================

  /**
   * Register script (memory-only, no persistence).
   * Used for predefined content registration during bootstrap.
   *
   * @param script - Script definition
   * @throws RegistryValidationError If the script definition is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  register(script: Script): void {
    this.validateScript(script);

    const scriptWithDefaults: Script = {
      ...script,
      enabled: script.enabled !== undefined ? script.enabled : true,
    };

    if (this.scripts.has(script.name)) {
      throw new RegistryAlreadyExistsError(script.name, "Script");
    }

    this.scripts.set(script.name, scriptWithDefaults);
    logger.info("Script registered (memory-only)", { scriptName: script.name });
  }

  /**
   * Register script with storage persistence (write-through).
   *
   * @param script - Script definition
   * @throws RegistryValidationError If the script definition is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  async registerScript(script: Script): Promise<void> {
    this.validateScript(script);

    const scriptWithDefaults: Script = {
      ...script,
      enabled: script.enabled !== undefined ? script.enabled : true,
    };

    if (this.scripts.has(script.name)) {
      throw new RegistryAlreadyExistsError(script.name, "Script");
    }

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistScript(scriptWithDefaults, this.storageAdapter);
    }

    this.scripts.set(script.name, scriptWithDefaults);
    logger.info("Script registered", { scriptName: script.name });
  }

  /**
   * Update script definition (memory-only).
   *
   * @param scriptName - Script name
   * @param updates - Update content
   * @throws RegistryNotFoundError If the script does not exist
   */
  update(scriptName: string, updates: Partial<Script>): void {
    const script = this.scripts.get(scriptName);
    if (!script) {
      throw new RegistryNotFoundError(scriptName, "Script");
    }

    const updatedScript = {
      ...script,
      ...updates,
      enabled: updates.enabled !== undefined ? updates.enabled : script.enabled ?? true,
    };

    this.validateScript(updatedScript);
    this.scripts.set(scriptName, updatedScript);
    logger.info("Script updated", { scriptName });
  }

  /**
   * Update script definition with storage persistence (write-through).
   *
   * @param scriptName - Script name
   * @param updates - Update content
   * @throws RegistryNotFoundError If the script does not exist
   */
  async updateScript(scriptName: string, updates: Partial<Script>): Promise<void> {
    const script = this.scripts.get(scriptName);
    if (!script) {
      throw new RegistryNotFoundError(scriptName, "Script");
    }

    const updatedScript = {
      ...script,
      ...updates,
      enabled: updates.enabled !== undefined ? updates.enabled : script.enabled ?? true,
    };

    this.validateScript(updatedScript);

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistScript(updatedScript, this.storageAdapter);
    }

    this.scripts.set(scriptName, updatedScript);
    logger.info("Script updated", { scriptName });
  }

  /**
   * Unregister script (memory-only).
   *
   * @param scriptName - Script name
   * @throws RegistryNotFoundError If the script does not exist
   */
  unregister(scriptName: string): void {
    if (!this.scripts.has(scriptName)) {
      throw new RegistryNotFoundError(scriptName, "Script");
    }
    this.scripts.delete(scriptName);
    logger.info("Script unregistered", { scriptName });
  }

  /**
   * Unregister script with storage persistence (write-through).
   *
   * @param scriptName - Script name
   * @throws RegistryNotFoundError If the script does not exist
   */
  async unregisterScript(scriptName: string): Promise<void> {
    if (!this.scripts.has(scriptName)) {
      throw new RegistryNotFoundError(scriptName, "Script");
    }

    // Remove from storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await removeScript(scriptName, this.storageAdapter);
    }

    this.scripts.delete(scriptName);
    logger.info("Script unregistered", { scriptName });
  }

  // ============================================================
  // BatchOperations Interface Implementation
  // ============================================================

  /**
   * Batch register scripts (memory-only).
   *
   * @param scripts - Array of script definitions
   */
  async registerBatch(scripts: Script[]): Promise<void> {
    for (const script of scripts) {
      this.register(script);
    }
  }

  /**
   * Batch unregister scripts (memory-only).
   *
   * @param keys - Array of script names
   */
  async unregisterBatch(keys: string[]): Promise<void> {
    for (const key of keys) {
      this.unregister(key);
    }
  }

  /**
   * Batch register scripts with storage persistence.
   *
   * @param scripts - Array of script definitions
   */
  async registerScripts(scripts: Script[]): Promise<void> {
    for (const script of scripts) {
      await this.registerScript(script);
    }
  }

  // ============================================================
  // SearchableRegistry Interface Implementation
  // ============================================================

  /**
   * Search scripts by keyword.
   *
   * @param query - Search keyword
   * @returns Array of matching scripts
   */
  search(query: string): Script[] {
    const lowerQuery = query.toLowerCase();
    return this.list().filter((script) => {
      return (
        script.name.toLowerCase().includes(lowerQuery) ||
        script.description.toLowerCase().includes(lowerQuery) ||
        script.metadata?.tags?.some((tag) => tag.toLowerCase().includes(lowerQuery)) ||
        script.metadata?.category?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * List scripts by category.
   *
   * @param category - Script category
   * @returns Array of script definitions
   */
  listByCategory(category: string): Script[] {
    return this.list().filter((script) => script.metadata?.category === category);
  }

  /**
   * List scripts by tags.
   *
   * @param tags - Array of tags
   * @returns Array of script definitions
   */
  listByTags(tags: string[]): Script[] {
    return this.list().filter((script) => {
      const scriptTags = script.metadata?.tags || [];
      return tags.every((tag) => scriptTags.includes(tag));
    });
  }

  // ============================================================
  // Additional Query Methods
  // ============================================================

  /**
   * Get script definition (throws if not found).
   *
   * @param scriptName - Script name
   * @returns Script definition
   * @throws ScriptNotFoundError If the script does not exist
   */
  getScript(scriptName: string): Script {
    const script = this.scripts.get(scriptName);
    if (!script) {
      throw new ScriptNotFoundError(`Script '${scriptName}' not found`, scriptName);
    }
    return script;
  }

  /**
   * Get script definition (may return undefined).
   *
   * @param scriptName - Script name
   * @returns Script definition or undefined
   */
  findScript(scriptName: string): Script | undefined {
    return this.scripts.get(scriptName);
  }

  /**
   * List scripts by category.
   *
   * @param category - Script category
   * @returns Array of script definitions
   */
  listScriptsByCategory(category: string): Script[] {
    return this.listByCategory(category);
  }

  /**
   * Check if script exists (alias for has).
   * Provided for backward compatibility.
   *
   * @param scriptName - Script name
   * @returns Whether it exists
   */
  hasScript(scriptName: string): boolean {
    return this.has(scriptName);
  }

  /**
   * List all scripts (alias for list).
   * Provided for backward compatibility.
   *
   * @returns Array of script definitions
   */
  listScripts(): Script[] {
    return this.list();
  }

  /**
   * Search scripts (alias for search).
   * Provided for backward compatibility.
   *
   * @param query - Search keyword
   * @returns Array of matching scripts
   */
  searchScripts(query: string): Script[] {
    return this.search(query);
  }

  /**
   * Clear all scripts (alias for clear).
   * Provided for backward compatibility.
   */
  clearScripts(): void {
    this.clear();
  }

  /**
   * Get the number of scripts (alias for size).
   * Provided for backward compatibility.
   *
   * @returns The number of scripts
   */
  scriptCount(): number {
    return this.scripts.size;
  }

  /**
   * Execute a script.
   * Delegates to ScriptExecutionService for actual execution.
   *
   * @param scriptName - Script name
   * @param options - Execution options
   * @returns Execution result
   * @throws ScriptNotFoundError If the script does not exist
   */
  async execute(
    scriptName: string,
    options: Partial<ScriptExecutionOptions> = {},
  ): Promise<Result<ScriptExecutionResult, ScriptExecutionError>> {
    const executionService = new ScriptExecutionService();
    return executionService.execute(scriptName, options, this);
  }

  /**
   * Execute a flow blueprint.
   * Delegates to ScriptExecutionService for actual execution.
   *
   * @param flowName - Flow name
   * @returns Flow execution result
   * @throws RegistryNotFoundError If the flow does not exist
   */
  async executeFlow(flowName: string): Promise<import("../../services/script/engine/script-flow-engine.js").FlowExecutionResult> {
    const executionService = new ScriptExecutionService();
    return executionService.executeFlow(flowName, this);
  }

  /**
   * Enable the script.
   *
   * @param scriptName - Script name
   * @throws RegistryNotFoundError If the script does not exist
   */
  async enableScript(scriptName: string): Promise<void> {
    await this.updateScript(scriptName, { enabled: true });
  }

  /**
   * Disable the script.
   *
   * @param scriptName - Script name
   * @throws RegistryNotFoundError If the script does not exist
   */
  async disableScript(scriptName: string): Promise<void> {
    await this.updateScript(scriptName, { enabled: false });
  }

  /**
   * Check if the script is enabled.
   *
   * @param scriptName - Script name
   * @returns Whether it is enabled
   * @throws RegistryNotFoundError If the script does not exist
   */
  isScriptEnabled(scriptName: string): boolean {
    const script = this.getScript(scriptName);
    return script.enabled ?? true;
  }

  // ============================================================
  // Flow Blueprint Management
  // ============================================================

  /**
   * Register a flow blueprint.
   *
   * @param flow - Flow blueprint definition
   * @throws RegistryAlreadyExistsError If the flow name already exists
   */
  registerFlow(flow: ScriptFlow): void {
    if (this.flows.has(flow.name)) {
      throw new RegistryAlreadyExistsError(flow.name, "Flow");
    }
    this.flows.set(flow.name, flow);
    logger.info("Flow registered", { flowName: flow.name });
  }

  /**
   * Get a flow blueprint.
   *
   * @param flowName - Flow name
   * @returns Flow blueprint
   * @throws RegistryNotFoundError If the flow does not exist
   */
  getFlow(flowName: string): ScriptFlow {
    const flow = this.flows.get(flowName);
    if (!flow) {
      throw new RegistryNotFoundError(flowName, "Flow");
    }
    return flow;
  }

  /**
   * List all registered flows.
   *
   * @returns Array of flow blueprints
   */
  listFlows(): ScriptFlow[] {
    return Array.from(this.flows.values());
  }

  // ============================================================
  // Storage Operations
  // ============================================================

  /**
   * Initialize scripts from storage.
   * Loads all persisted script definitions into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeScriptsFromStorage(this.storageAdapter, this.scripts);
  }

  // ============================================================
  // Validation
  // ============================================================

  /**
   * Validate script definition.
   *
   * @param script - The script definition to validate
   * @returns Whether it is valid
   * @throws RegistryValidationError If the script definition is invalid
   */
  validateScript(script: Script): boolean {
    // Validate required fields using standardized validators
    validateRequiredString(script as unknown as Record<string, unknown>, "name", "Script name is required and must be a string");
    validateRequiredString(script as unknown as Record<string, unknown>, "description", "Script description is required and must be a string");

    // Validate at least one content source is provided
    validateAtLeastOne(
      script as unknown as Record<string, unknown>,
      ["content", "filePath", "template"],
      "Script must have either content, filePath, or template",
    );

    // Validate options
    if (!script.options) {
      throw new RegistryValidationError("Script options are required", "options");
    }

    // Validate numeric options
    if (script.options.timeout !== undefined) {
      validatePositiveNumber(script.options as Record<string, unknown>, "timeout", "Script timeout must be a non-negative number");
    }

    if (script.options.retries !== undefined) {
      validatePositiveNumber(script.options as Record<string, unknown>, "retries", "Script retries must be a non-negative number");
    }

    if (script.options.retryDelay !== undefined) {
      validatePositiveNumber(script.options as Record<string, unknown>, "retryDelay", "Script retryDelay must be a non-negative number");
    }

    // Validate enabled flag
    if (script.enabled !== undefined) {
      validateBoolean(script as unknown as Record<string, unknown>, "enabled", "Script enabled must be a boolean");
    }

    return true;
  }
}

/**
 * Script Execution Service
 * Handles script and flow execution, independent of registry concerns.
 *
 * Responsibilities:
 * - Script execution (simple, engine-based, batch)
 * - Flow blueprint execution
 */
class ScriptExecutionService {
  private scriptEngine: ScriptEngine | null = null;
  private flowEngine: ScriptFlowEngine | null = null;

  constructor(
    private readonly executor: ScriptExecutor_ = new ScriptExecutor_(),
  ) {}

  /**
   * Execute the script.
   *
   * @param scriptName - Script name
   * @param options - Execution options
   * @param registry - ScriptRegistry instance to look up script definitions
   */
  async execute(
    scriptName: string,
    options: Partial<ScriptExecutionOptions> = {},
    registry: ScriptRegistry,
  ): Promise<Result<ScriptExecutionResult, ScriptExecutionError>> {
    logger.debug("Script execution started", { scriptName });

    const script = registry.getScript(scriptName);
    const result = await this.executor.execute(script, options);

    if (!result.success) {
      return err(
        new ScriptExecutionError(result.error || "Script execution failed", scriptName, {
          options,
        }),
      );
    }

    logger.debug("Script execution completed", { scriptName, success: result.success });
    return ok(result);
  }

  /**
   * Execute the script with ScriptEngine (supports template + executor mode).
   *
   * @param scriptName - Script name
   * @param options - Execution options
   * @param args - Runtime argument values for template rendering
   * @param registry - ScriptRegistry instance to look up script definitions
   */
  async executeWithEngine(
    scriptName: string,
    options: Partial<ScriptExecutionOptions> = {},
    args: Record<string, unknown> = {},
    registry: ScriptRegistry,
  ): Promise<Result<ScriptExecutionResult, ScriptExecutionError>> {
    const script = registry.getScript(scriptName);

    if (!this.scriptEngine) {
      this.scriptEngine = new ScriptEngine();
    }

    const result = await this.scriptEngine.execute(script, options, { args });

    if (!result.success) {
      return err(
        new ScriptExecutionError(result.error || "Script execution failed", scriptName, {
          options,
          args,
        }),
      );
    }

    return ok(result);
  }

  /**
   * Execute a flow blueprint.
   *
   * @param flowName - Flow name
   * @param registry - ScriptRegistry instance to look up flow and script definitions
   */
  async executeFlow(
    flowName: string,
    registry: ScriptRegistry,
  ): Promise<import("../../services/script/engine/script-flow-engine.js").FlowExecutionResult> {
    const flow = registry.getFlow(flowName);

    if (!this.scriptEngine) {
      this.scriptEngine = new ScriptEngine();
    }
    if (!this.flowEngine) {
      this.flowEngine = new ScriptFlowEngine(this.scriptEngine, registry["scripts"] as unknown as Map<string, Script>);
    }

    return this.flowEngine.execute(flow);
  }

  /**
   * Batch execute scripts.
   *
   * @param executions - Execution task array
   * @param registry - ScriptRegistry instance to look up script definitions
   */
  async executeBatch(
    executions: Array<{
      scriptName: string;
      options?: Partial<ScriptExecutionOptions>;
    }>,
    registry: ScriptRegistry,
  ): Promise<Result<ScriptExecutionResult[], ScriptExecutionError>> {
    const results = await Promise.all(
      executions.map((exec) => this.execute(exec.scriptName, exec.options, registry)),
    );

    return all(results);
  }
}

export { ScriptRegistry, ScriptExecutionService };

