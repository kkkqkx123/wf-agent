/**
 * Tool Registrys
 * Provides a unified interface for tool execution and tool registration management
 *
 * This module only exports class definitions; instances are managed uniformly through SingletonRegistry.
 *
 */

import type { Tool } from "@wf-agent/types";
import {
  ToolError,
  ToolNotFoundError,
  RuntimeValidationError,
  ConfigurationValidationError,
} from "@wf-agent/types";
import type { IToolExecutor } from "@wf-agent/tool-executors";
import type { ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";
import { StatelessExecutor } from "@wf-agent/tool-executors";
import { StatefulExecutor } from "@wf-agent/tool-executors";
import { RestExecutor } from "@wf-agent/tool-executors";
import { BuiltinExecutor } from "@wf-agent/tool-executors";
import { tryCatchAsyncWithSignal, all } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { StaticValidator } from "../validation/tool-static-validator.js";
import { RuntimeValidator } from "../validation/tool-runtime-validator.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { createBuiltinTools } from "../../resources/predefined/tools/builtin/index.js";

const logger = createContextualLogger({ component: "ToolRegistry" });

/**
 * Tool Registry Class
 */
class ToolRegistry {
  private tools: Map<string, Tool> = new Map();
  private executors: Map<string, IToolExecutor> = new Map();
  private staticValidator: StaticValidator;
  private runtimeValidator: RuntimeValidator;
  private builtinExecutor: BuiltinExecutor;

  constructor() {
    this.staticValidator = new StaticValidator();
    this.runtimeValidator = new RuntimeValidator();
    this.builtinExecutor = new BuiltinExecutor();
    this.initializeExecutors();
  }

  /**
   * Initialize the executor.
   */
  private initializeExecutors(): void {
    // Use the implementations directly from the packages.
    this.executors.set("STATELESS", new StatelessExecutor());
    this.executors.set("STATEFUL", new StatefulExecutor());
    this.executors.set("REST", new RestExecutor());
    this.executors.set("BUILTIN", this.builtinExecutor);
  }

  /**
   * Register Tool
   * @param tool Tool definition
   * @param options Registration options
   * @throws ConfigurationValidationError If the tool definition is invalid or already exists
   */
  registerTool(tool: Tool, options?: { skipIfExists?: boolean }): void {
    // Static Validation Tool Definition
    const result = this.staticValidator.validateTool(tool);
    if (result.isErr()) {
      throw result.error[0];
    }

    // Check if the tool ID already exists.
    if (this.tools.has(tool.id)) {
      if (options?.skipIfExists) {
        logger.info("Tool already exists, skipping", { toolId: tool.id });
        return;
      }
      logger.warn("Tool already exists", { toolId: tool.id });
      throw new ConfigurationValidationError(`Tool with id '${tool.id}' already exists`, {
        configType: "tool",
        field: "id",
        value: tool.id,
      });
    }

    this.tools.set(tool.id, tool);
    logger.info("Tool registered", { toolId: tool.id, toolType: tool.type, toolName: tool.name });
  }

  /**
   * Batch Registration Tool
   * @param tools: An array of tool definitions
   * @param options: Registration options
   */
  registerTools(tools: Tool[], options?: { skipIfExists?: boolean }): void {
    for (const tool of tools) {
      this.registerTool(tool, options);
    }
  }

  /**
   * Tool Deactivation
   * @param toolId Tool ID
   * @throws ToolNotFoundError If the tool does not exist
   */
  unregisterTool(toolId: string): void {
    if (!this.tools.has(toolId)) {
      logger.warn("Attempted to unregister non-existent tool", { toolId });
      throw new ToolNotFoundError(`Tool with id '${toolId}' not found`, toolId);
    }
    this.tools.delete(toolId);
    logger.info("Tool unregistered", { toolId });
  }

  /**
   * Get Tool Definition
   * @param toolId Tool ID
   * @returns Tool Definition
   * @throws ToolNotFoundError If the tool does not exist
   */
  getTool(toolId: string): Tool {
    const tool = this.tools.get(toolId);
    if (!tool) {
      throw new ToolNotFoundError(`Tool with id '${toolId}' not found`, toolId);
    }
    return tool;
  }

  /**
   * Check if the tool exists
   * @param toolId Tool ID
   * @returns Whether it exists
   */
  has(toolId: string): boolean {
    return this.tools.has(toolId);
  }

  /**
   * List all tools
   * @returns An array of tool definitions
   */
  listTools(): Tool[] {
    return Array.from(this.tools.values());
  }

  /**
   * List tools by type
   * @param type: Tool type
   * @returns: Array of tool definitions
   */
  listToolsByType(type: string): Tool[] {
    return this.listTools().filter(tool => tool.type === type);
  }

  /**
   * List tools by category
   * @param category Tool category
   * @returns Array of tool definitions
   */
  listToolsByCategory(category: string): Tool[] {
    return this.listTools().filter(tool => tool.metadata?.category === category);
  }

  /**
   * Search Tool
   * @param query Search keyword
   * @returns Array of matching tools
   */
  searchTools(query: string): Tool[] {
    const lowerQuery = query.toLowerCase();
    return this.listTools().filter(tool => {
      return (
        tool.id.toLowerCase().includes(lowerQuery) ||
        tool.description.toLowerCase().includes(lowerQuery) ||
        tool.metadata?.tags?.some(tag => tag.toLowerCase().includes(lowerQuery)) ||
        tool.metadata?.category?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * Check if the tool exists
   * @param toolId Tool ID
   * @returns Whether it exists
   */
  hasTool(toolId: string): boolean {
    return this.tools.has(toolId);
  }

  /**
   * Get the number of tools
   * @returns The number of tools
   */
  size(): number {
    return this.tools.size;
  }

  /**
   * Execution Tool
   * @param toolId: Tool ID
   * @param parameters: Tool parameters
   * @param options: Execution options
   * @param executionId: Execution ID (optional, for stateful tools)
   * @returns: Result<ToolExecutionResult, ToolError>
   */
  async execute(
    toolId: string,
    parameters: Record<string, unknown>,
    options: ToolExecutionOptions = {},
    executionId?: string,
  ): Promise<Result<ToolExecutionResult, ToolError>> {
    logger.debug("Tool execution started", {
      toolId,
      executionId,
      hasParameters: Object.keys(parameters).length > 0,
    });

    // Obtain tool definitions
    const tool = this.getTool(toolId);

    // Get the corresponding executor
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

    // Use `tryCatchAsyncWithSignal` to ensure that the signal is passed correctly.
    const result = await tryCatchAsyncWithSignal(
      (signal: AbortSignal | undefined) =>
        executor.execute(tool, parameters, { ...options, signal }, executionId),
      options?.signal,
    );

    if (result.isErr()) {
      return err(this.convertToToolError(result.error, toolId, tool.type, parameters));
    }

    logger.debug("Tool execution completed", { toolId, success: result.value.success });
    return ok(result.value);
  }

  /**
   * 批量执行工具
   * @param executions 执行任务数组
   * @param executionId 线程 ID（可选，用于有状态工具）
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
    // Execute all tools in parallel.
    const results = await Promise.all(
      executions.map(exec => this.execute(exec.toolId, exec.parameters, exec.options, executionId)),
    );

    // Combine the results; return success if everything is successful, otherwise return the first error.
    return all(results);
  }

  /**
   * Verify tool parameters (runtime validation)
   * @param toolId: Tool ID
   * @param parameters: Tool parameters
   * @returns: Validation result
   */
  validateParameters(
    toolId: string,
    parameters: Record<string, unknown>,
  ): { valid: boolean; errors: string[] } {
    try {
      const tool = this.getTool(toolId);

      // Use a runtime validator.
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

  /**
   * Clear all tools
   */
  clear(): void {
    const count = this.tools.size;
    this.tools.clear();
    logger.info("All tools cleared", { count });
  }

  /**
   * Clean up all stateful tool instances for the specified execution.
   * @param executionId: Execution ID
   */
  cleanupWorkflowExecution(executionId: string): void {
    logger.debug("Cleaning up execution stateful tools", { executionId });
    const statefulExecutor = this.executors.get("STATEFUL");
    if (
      statefulExecutor &&
      typeof (statefulExecutor as { cleanupWorkflowExecution?: (executionId: string) => void }).cleanupWorkflowExecution ===
        "function"
    ) {
      (statefulExecutor as unknown as { cleanupWorkflowExecution: (executionId: string) => void }).cleanupWorkflowExecution(
        executionId,
      );
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

  /**
   * Update tool definition
   * @param toolId Tool ID
   * @param updates Update content
   * @throws ToolNotFoundError If the tool does not exist
   */
  updateTool(toolId: string, updates: Partial<Tool>): void {
    const tool = this.getTool(toolId);
    const updatedTool = { ...tool, ...updates };
    // Delete the old tool first, then register the new one (re-verification will be required).
    this.tools.delete(toolId);
    this.registerTool(updatedTool);
  }

  /**
   * Translate from auto to en:
   *
   * Translate error message to ToolError
   *
   * @param error The original error message
   * @param toolId The tool ID
   * @param toolType The type of the tool
   */
  private convertToToolError(
    error: unknown,
    toolId: string,
    toolType: string,
    parameters: Record<string, unknown>,
  ): ToolError {
    // If it's already a ToolError, return it directly.
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

  /**
   * Get all available tools (including built-in tools)
   * @param customTools Custom tools to include
   * @returns Array of all available tools
   */
  getAvailableTools(customTools: Tool[] = []): Tool[] {
    const builtinTools = createBuiltinTools();
    return [...customTools, ...builtinTools];
  }

  /**
   * Get built-in tools only
   * @returns Array of built-in tools
   */
  getBuiltinTools(): Tool[] {
    return createBuiltinTools();
  }

  /**
   * Update the builtin executor context
   * This allows setting context information for builtin tool execution
   * @param context Context to update
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
}

/**
 * Export the ToolRegistry class
 */
export { ToolRegistry };
