/**
 * Tool Registry
 *
 * Provides a unified interface for tool execution and tool registration management.
 * Implements standardized registry interfaces for consistency.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 *
 * Interface Implementation:
 * - Registry<Tool>: Read operations
 * - MutableRegistry<Tool>: Write operations
 * - BatchOperations<Tool>: Batch register/unregister
 * - SearchableRegistry<Tool>: Search and filter operations
 */

import type { Tool } from "@wf-agent/types";
import {
  ToolError,
  ToolNotFoundError,
  RuntimeValidationError,
} from "@wf-agent/types";
import type { IToolExecutor } from "../../services/tools/core/interfaces.js";
import type { ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";
import type { RestExecutorConfig } from "../../services/tools/executors/rest.js";
import { StatelessExecutor } from "../../services/tools/executors/stateless.js";
import { StatefulExecutor } from "../../services/tools/executors/stateful.js";
import { RestExecutor } from "../../services/tools/executors/rest.js";
import { BuiltinExecutor } from "../../services/tools/executors/builtin.js";
import { McpExecutor } from "../../services/tools/executors/mcp.js";
import { tryCatchAsyncWithSignal, all } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { StaticValidator } from "../validation/tool-static-validator.js";
import { RuntimeValidator } from "../validation/tool-runtime-validator.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { createBuiltinTools } from "../../resources/predefined/tools/builtin/index.js";
import type { ToolStorageAdapter } from "@wf-agent/storage";
import { persistTool, removeTool, initializeToolsFromStorage } from "./utils/storage/index.js";
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
} from "./types.js";

const logger = createContextualLogger({ component: "ToolRegistry" });

/**
 * Tool Registry Class
 *
 * Implements:
 * - Registry<Tool>: Read operations (get, has, list, keys, size, clear)
 * - MutableRegistry<Tool>: Write operations (set, delete)
 * - BatchOperations<Tool>: Batch register/unregister
 * - SearchableRegistry<Tool>: Search and filter
 */
class ToolRegistry
  implements
    Registry<Tool>,
    MutableRegistry<Tool>,
    BatchOperations<Tool>,
    SearchableRegistry<Tool>
{
  private items = createRegistry<Tool>();
  private executors: Map<string, IToolExecutor> = new Map();
  private staticValidator: StaticValidator;
  private runtimeValidator: RuntimeValidator;
  private builtinExecutor: BuiltinExecutor;
  private restExecutorConfig: RestExecutorConfig;

  // Plugin-contributed executor constructors (registered via contribution manager)
  private pluginExecutorConstructors: Map<string, { executor: new (...args: unknown[]) => IToolExecutor; pluginId: string }> = new Map();

  // Tool availability management for execution-level scoping
  private toolAvailability: Map<string, {
    available: boolean;
    reason?: string;
    restrictions?: {
      executionIds?: string[];
      excluded?: string[];
    };
  }> = new Map();

  private availabilityObservers: Set<(change: {
    toolId: string;
    available: boolean;
    reason?: string;
  }) => void> = new Set();

  constructor(
    restExecutorConfig: RestExecutorConfig = {},
    private readonly storageAdapter: ToolStorageAdapter | null = null,
  ) {
    this.staticValidator = new StaticValidator();
    this.runtimeValidator = new RuntimeValidator();
    this.builtinExecutor = new BuiltinExecutor();
    this.restExecutorConfig = restExecutorConfig;
    this.initializeExecutors();
  }

  /**
   * Initialize the executors.
   */
  private initializeExecutors(): void {
    this.executors.set("STATELESS", new StatelessExecutor());
    this.executors.set("STATEFUL", new StatefulExecutor());
    this.executors.set("REST", new RestExecutor(this.restExecutorConfig));
    this.executors.set("BUILTIN", this.builtinExecutor);
    this.executors.set("MCP", new McpExecutor());
  }

  /**
   * Register a plugin-contributed executor constructor.
   * The executor will be instantiated lazily when first needed.
   */
  registerPluginExecutor(type: string, executorConstructor: new (...args: unknown[]) => IToolExecutor, pluginId: string): void {
    this.pluginExecutorConstructors.set(type, { executor: executorConstructor, pluginId });
  }

  /**
   * Initialize executors from plugin-contributed constructors.
   * Called after all plugins are loaded and activated.
   */
  initializePluginExecutors(): void {
    for (const [type, entry] of this.pluginExecutorConstructors) {
      if (!this.executors.has(type)) {
        try {
          this.executors.set(type, new entry.executor());
        } catch (error) {
          // Log error but continue with other executors
          console.error(`Failed to instantiate plugin executor '${type}' from plugin '${entry.pluginId}':`, error);
        }
      }
    }
  }

  // ============================================================
  // Registry Interface Implementation (Read Operations)
  // ============================================================

  /** Get tool by ID, returns undefined if not found */
  get(key: string): Tool | undefined {
    return this.items.get(key);
  }

  /** Check if tool exists */
  has(key: string): boolean {
    return this.items.has(key);
  }

  /** List all tools */
  list(): Tool[] {
    return this.items.list();
  }

  /** Get all tool IDs */
  keys(): string[] {
    return this.items.keys();
  }

  /** Get the number of tools */
  get size(): number {
    return this.items.size;
  }

  /** Clear all tools */
  async clear(): Promise<void> {
    const count = this.items.size;
    this.items.clear();
    if (this.storageAdapter) {
      await this.storageAdapter.clear();
    }
    logger.info("All tools cleared", { count });
  }

  // ============================================================
  // MutableRegistry Interface Implementation (Write Operations)
  // ============================================================

  /** Set a tool by ID */
  set(key: string, value: Tool): void {
    this.items.set(key, value);
  }

  /** Delete a tool by ID, returns true if deleted */
  delete(key: string): boolean {
    return this.items.delete(key);
  }

  // ============================================================
  // Core CRUD Operations (Standardized Naming)
  // ============================================================

  /**
   * Register tool (memory-only, no persistence).
   * Used for predefined content registration during bootstrap.
   *
   * @param tool - Tool definition
   * @param options - Registration options
   * @throws ConfigurationValidationError If the tool definition is invalid
   * @throws RegistryAlreadyExistsError If the tool ID already exists
   */
  register(tool: Tool, options?: { skipIfExists?: boolean }): void {
    const result = this.staticValidator.validateTool(tool);
    if (result.isErr()) {
      throw result.error[0];
    }

    if (this.items.has(tool.id)) {
      if (options?.skipIfExists) {
        logger.info("Tool already exists, skipping", { toolId: tool.id });
        return;
      }
      throw new RegistryAlreadyExistsError(tool.id, "Tool");
    }

    this.items.set(tool.id, tool);
    logger.info("Tool registered (memory-only)", { toolId: tool.id, toolType: tool.type });
  }

  /**
   * Register tool with storage persistence (write-through).
   *
   * @param tool - Tool definition
   * @param options - Registration options
   * @throws ConfigurationValidationError If the tool definition is invalid
   * @throws RegistryAlreadyExistsError If the tool ID already exists
   */
  async registerTool(tool: Tool, options?: { skipIfExists?: boolean }): Promise<void> {
    const result = this.staticValidator.validateTool(tool);
    if (result.isErr()) {
      throw result.error[0];
    }

    if (this.items.has(tool.id)) {
      if (options?.skipIfExists) {
        logger.info("Tool already exists, skipping", { toolId: tool.id });
        return;
      }
      throw new RegistryAlreadyExistsError(tool.id, "Tool");
    }

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistTool(tool, this.storageAdapter);
    }

    this.items.set(tool.id, tool);
    logger.info("Tool registered", { toolId: tool.id, toolType: tool.type });
  }

  /**
   * Update tool definition (memory-only).
   *
   * @param toolId - Tool ID
   * @param updates - Update content
   * @throws RegistryNotFoundError If the tool does not exist
   */
  update(toolId: string, updates: Partial<Tool>): void {
    const tool = this.items.get(toolId);
    if (!tool) {
      throw new RegistryNotFoundError(toolId, "Tool");
    }

    const updatedTool = { ...tool, ...updates, id: toolId };
    this.items.set(toolId, updatedTool);
    logger.info("Tool updated", { toolId });
  }

  /**
   * Update tool definition with storage persistence (write-through).
   *
   * @param toolId - Tool ID
   * @param updates - Update content
   * @throws RegistryNotFoundError If the tool does not exist
   * @throws ConfigurationValidationError If the updated tool is invalid
   */
  async updateTool(toolId: string, updates: Partial<Tool>): Promise<void> {
    const tool = this.items.get(toolId);
    if (!tool) {
      throw new RegistryNotFoundError(toolId, "Tool");
    }

    const updatedTool = { ...tool, ...updates, id: toolId };

    // Re-validate the updated tool
    const result = this.staticValidator.validateTool(updatedTool);
    if (result.isErr()) {
      throw result.error[0];
    }

    // Persist to storage first (write-through)
    if (this.storageAdapter) {
      await persistTool(updatedTool, this.storageAdapter);
    }

    this.items.set(toolId, updatedTool);
    logger.info("Tool updated", { toolId });
  }

  /**
   * Unregister tool (memory-only).
   *
   * @param toolId - Tool ID
   * @throws RegistryNotFoundError If the tool does not exist
   */
  unregister(toolId: string): void {
    if (!this.items.has(toolId)) {
      throw new RegistryNotFoundError(toolId, "Tool");
    }
    this.items.delete(toolId);
    logger.info("Tool unregistered", { toolId });
  }

  /**
   * Unregister tool with storage persistence (write-through).
   *
   * @param toolId - Tool ID
   * @throws RegistryNotFoundError If the tool does not exist
   */
  async unregisterTool(toolId: string): Promise<void> {
    if (!this.items.has(toolId)) {
      throw new RegistryNotFoundError(toolId, "Tool");
    }

    // Remove from storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await removeTool(toolId, this.storageAdapter);
    }

    this.items.delete(toolId);
    logger.info("Tool unregistered", { toolId });
  }

  // ============================================================
  // BatchOperations Interface Implementation
  // ============================================================

  /**
   * Batch register tools (memory-only).
   *
   * @param tools - Array of tool definitions
   * @param options - Registration options
   */
  async registerBatch(tools: Tool[], options?: { skipIfExists?: boolean }): Promise<void> {
    for (const tool of tools) {
      this.register(tool, options);
    }
  }

  /**
   * Batch unregister tools (memory-only).
   *
   * @param keys - Array of tool IDs
   */
  async unregisterBatch(keys: string[]): Promise<void> {
    for (const key of keys) {
      this.unregister(key);
    }
  }

  /**
   * Batch register tools with storage persistence.
   *
   * @param tools - Array of tool definitions
   * @param options - Registration options
   */
  async registerTools(tools: Tool[], options?: { skipIfExists?: boolean }): Promise<void> {
    for (const tool of tools) {
      await this.registerTool(tool, options);
    }
  }

  // ============================================================
  // SearchableRegistry Interface Implementation
  // ============================================================

  /**
   * Search tools by keyword.
   *
   * @param query - Search keyword
   * @returns Array of matching tools
   */
  search(query: string): Tool[] {
    const lowerQuery = query.toLowerCase();
    return this.list().filter((tool) => {
      return (
        tool.id.toLowerCase().includes(lowerQuery) ||
        tool.description.toLowerCase().includes(lowerQuery) ||
        tool.metadata?.tags?.some((tag) => tag.toLowerCase().includes(lowerQuery)) ||
        tool.metadata?.category?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * List tools by category.
   *
   * @param category - Tool category
   * @returns Array of tool definitions
   */
  listByCategory(category: string): Tool[] {
    return this.list().filter((tool) => tool.metadata?.category === category);
  }

  /**
   * List tools by tags.
   *
   * @param tags - Array of tags
   * @returns Array of tool definitions
   */
  listByTags(tags: string[]): Tool[] {
    return this.list().filter((tool) => {
      const toolTags = tool.metadata?.tags || [];
      return tags.every((tag) => toolTags.includes(tag));
    });
  }

  // ============================================================
  // Additional Query Methods
  // ============================================================

  /**
   * Get tool definition (throws if not found).
   *
   * @param toolId - Tool ID
   * @returns Tool definition
   * @throws ToolNotFoundError If the tool does not exist
   */
  getTool(toolId: string): Tool {
    const tool = this.items.get(toolId);
    if (!tool) {
      throw new ToolNotFoundError(`Tool with id '${toolId}' not found`, toolId);
    }
    return tool;
  }

  /**
   * Check if tool exists (alias for has).
   * Provided for backward compatibility.
   *
   * @param toolId - Tool ID
   * @returns Whether it exists
   */
  hasTool(toolId: string): boolean {
    return this.has(toolId);
  }

  /**
   * List all tools (alias for list).
   * Provided for backward compatibility.
   *
   * @returns Array of tool definitions
   */
  listTools(): Tool[] {
    return this.list();
  }

  /**
   * List tools by type.
   *
   * @param type - Tool type
   * @returns Array of tool definitions
   */
  listByType(type: string): Tool[] {
    return this.list().filter((tool) => tool.type === type);
  }

  /**
   * List tools by type (alias for listByType).
   * Provided for backward compatibility.
   *
   * @param type - Tool type
   * @returns Array of tool definitions
   */
  listToolsByType(type: string): Tool[] {
    return this.listByType(type);
  }

  /**
   * List tools by category.
   * Provided for backward compatibility.
   *
   * @param category - Tool category
   * @returns Array of tool definitions
   */
  listToolsByCategory(category: string): Tool[] {
    return this.listByCategory(category);
  }

  /**
   * Search tools (alias for search).
   * Provided for backward compatibility.
   *
   * @param query - Search keyword
   * @returns Array of matching tools
   */
  searchTools(query: string): Tool[] {
    return this.search(query);
  }

  // ============================================================
  // Tool Execution
  // ============================================================

  /**
   * Execute tool.
   *
   * @param toolId - Tool ID
   * @param parameters - Tool parameters
   * @param options - Execution options
   * @param executionId - Execution ID (optional, for stateful tools)
   * @param context - Execution context (optional, for interactive tools)
   * @returns Result<ToolExecutionResult, ToolError>
   */
  async execute(
    toolId: string,
    parameters: Record<string, unknown>,
    options: ToolExecutionOptions = {},
    executionId?: string,
    context?: Record<string, unknown>,
  ): Promise<Result<ToolExecutionResult, ToolError>> {
    logger.debug("Tool execution started", {
      toolId,
      executionId,
      hasParameters: Object.keys(parameters).length > 0,
    });

    const tool = this.getTool(toolId);

    const executor = this.executors.get(tool.type);
    if (!executor) {
      return err(
        new ToolError(`No executor found for tool type '${tool.type}'`, toolId, tool.type, {
          parameters,
        }),
      );
    }

    // Runtime parameter validation
    try {
      this.runtimeValidator.validate(tool, parameters);
    } catch (error) {
      if (error instanceof RuntimeValidationError) {
        logger.warn("Tool parameter validation failed", { toolId, error: error.message });
        return err(new ToolError(error.message, toolId, tool.type, { parameters }, error));
      }
      logger.warn("Tool parameter validation failed", { toolId, error: String(error) });
      return err(
        new ToolError(
          "Parameter validation failed",
          toolId,
          tool.type,
          { parameters },
          error instanceof Error ? error : undefined,
        ),
      );
    }

    const result = await tryCatchAsyncWithSignal(
      (signal: AbortSignal | undefined) =>
        executor.execute(tool, parameters, { ...options, signal }, executionId, context),
      options?.signal,
    );

    if (result.isErr()) {
      return err(this.convertToToolError(result.error, toolId, tool.type, parameters));
    }

    logger.debug("Tool execution completed", { toolId, success: (result.value as ToolExecutionResult).success });
    return ok(result.value as ToolExecutionResult);
  }

  /**
   * Batch execute tools.
   *
   * @param executions - Execution task array
   * @param executionId - Execution ID (optional, for stateful tools)
   * @returns Result<ToolExecutionResult[], ToolError>
   */
  async executeBatch(
    executions: Array<{
      toolId: string;
      parameters: Record<string, unknown>;
      options?: ToolExecutionOptions;
    }>,
    executionId?: string,
  ): Promise<Result<ToolExecutionResult[], ToolError>> {
    const results = await Promise.all(
      executions.map((exec) => this.execute(exec.toolId, exec.parameters, exec.options, executionId)),
    );

    return all(results);
  }

  /**
   * Verify tool parameters (runtime validation).
   *
   * @param toolId - Tool ID
   * @param parameters - Tool parameters
   * @returns Validation result
   */
  validateParameters(
    toolId: string,
    parameters: Record<string, unknown>,
  ): { valid: boolean; errors: string[] } {
    try {
      const tool = this.getTool(toolId);

      try {
        this.runtimeValidator.validate(tool, parameters);
        return { valid: true, errors: [] };
      } catch (error) {
        return {
          valid: false,
          errors: [error instanceof Error ? error.message : "Unknown validation error"],
        };
      }
    } catch (error) {
      return {
        valid: false,
        errors: [error instanceof Error ? error.message : "Unknown error"],
      };
    }
  }

  // ============================================================
  // Lifecycle Management
  // ============================================================

  /**
   * Clean up all stateful tool instances for the specified execution.
   *
   * @param executionId - Execution ID
   */
  cleanupWorkflowExecution(executionId: string): void {
    logger.debug("Cleaning up execution stateful tools", { executionId });
    const statefulExecutor = this.executors.get("STATEFUL");
    if (
      statefulExecutor &&
      typeof (statefulExecutor as { cleanupWorkflowExecution?: (executionId: string) => void })
        .cleanupWorkflowExecution === "function"
    ) {
      (
        statefulExecutor as unknown as { cleanupWorkflowExecution: (executionId: string) => void }
      ).cleanupWorkflowExecution(executionId);
      logger.debug("Workflow execution stateful tools cleaned up", { executionId });
    }
  }

  /**
   * Clean up resources for all executors.
   */
  async cleanupAll(): Promise<void> {
    logger.info("Cleaning up all tool executors");
    for (const executor of this.executors.values()) {
      if (typeof executor.cleanup === "function") {
        await executor.cleanup();
      }
    }
    logger.info("All tool executors cleaned up");
  }

  // ============================================================
  // Built-in Tools
  // ============================================================

  /**
   * Get all available tools (including built-in tools).
   *
   * @param customTools - Custom tools to include
   * @returns Array of all available tools
   */
  getAvailableTools(customTools: Tool[] = []): Tool[] {
    const builtinTools = createBuiltinTools();
    return [...customTools, ...builtinTools];
  }

  /**
   * Get built-in tools only.
   *
   * @returns Array of built-in tools
   */
  getBuiltinTools(): Tool[] {
    return createBuiltinTools();
  }

  /**
   * Update the builtin executor context.
   *
   * @param context - Context to update
   */
  updateBuiltinContext(context: {
    executionId?: string;
    parentWorkflowExecutionEntity?: unknown;
    executionRegistry?: unknown;
    eventManager?: unknown;
    executionBuilder?: unknown;
    taskQueueManager?: unknown;
  }): void {
    this.builtinExecutor.updateDefaultContext(context);
  }

  // ============================================================
  // Storage Operations
  // ============================================================

  /**
   * Initialize tools from storage.
   * Loads all persisted tool definitions into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeToolsFromStorage(this.storageAdapter, this.items);
  }

  // ============================================================
  // Private Methods
  // ============================================================

  /**
   * Convert error to ToolError.
   *
   * @param error - The original error
   * @param toolId - The tool ID
   * @param toolType - The type of the tool
   * @param parameters - Tool parameters
   * @returns ToolError
   */
  private convertToToolError(
    error: unknown,
    toolId: string,
    toolType: string,
    parameters: Record<string, unknown>,
  ): ToolError {
    if (error instanceof ToolError) {
      return error;
    }

    const message = error instanceof Error ? error.message : String(error);

    return new ToolError(
      `Tool execution failed: ${message}`,
      toolId,
      toolType,
      { parameters },
      error instanceof Error ? error : undefined,
    );
  }

  // ============================================================
  // Tool Availability Management
  // ============================================================

  /**
   * Get available tools for a specific execution
   * @param executionId - Execution ID for filtering
   * @returns Array of available tools for this execution
   */
  getAvailableFor(executionId: string): Tool[] {
    return this.list().filter(tool => this.isAvailableFor(tool.id, executionId));
  }

  /**
   * Check if a tool is available for a specific execution
   * @param toolId - Tool ID
   * @param executionId - Execution ID
   * @returns true if the tool is available for this execution
   */
  isAvailableFor(toolId: string, executionId: string): boolean {
    const availability = this.toolAvailability.get(toolId);

    // If no availability record, tool is available by default
    if (!availability) return true;

    // If explicitly marked as unavailable, it's not available
    if (availability.available === false) return false;

    // Check execution-specific restrictions
    if (availability.restrictions?.executionIds) {
      return availability.restrictions.executionIds.includes(executionId);
    }

    if (availability.restrictions?.excluded) {
      return !availability.restrictions.excluded.includes(executionId);
    }

    return true;
  }

  /**
   * Set tool availability
   * @param toolId - Tool ID
   * @param available - Whether the tool is available
   * @param options - Additional availability options
   */
  setAvailability(
    toolId: string,
    available: boolean,
    options?: {
      reason?: string;
      restrictions?: {
        executionIds?: string[];
        excluded?: string[];
      };
    },
  ): void {
    const availability = {
      available,
      reason: options?.reason,
      restrictions: options?.restrictions,
    };

    this.toolAvailability.set(toolId, availability);

    // Notify observers
    this.availabilityObservers.forEach(observer => {
      observer({
        toolId,
        available,
        reason: options?.reason,
      });
    });

    logger.debug("Tool availability updated", {
      toolId,
      available,
      reason: options?.reason,
    });
  }

  /**
   * Subscribe to tool availability changes
   * @param callback - Callback function for availability changes
   * @returns Unsubscribe function
   */
  onAvailabilityChange(
    callback: (change: { toolId: string; available: boolean; reason?: string }) => void,
  ): () => void {
    this.availabilityObservers.add(callback);
    return () => this.availabilityObservers.delete(callback);
  }
}

/**
 * Export the ToolRegistry class
 */
export { ToolRegistry };

