/**
 * Stateless Tool Executor
 * Executes stateless functions provided by the application layer, manages functions through a function registry, and supports version control and call statistics.
 */

import type { Tool, ID, ToolOutput } from "@wf-agent/types";
import type { StatelessToolConfig } from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";
import { BaseExecutor } from "../core/base/BaseExecutor.js";
import { FunctionRegistry } from "./registry/FunctionRegistry.js";
import type { FunctionRegistryConfig } from "./types.js";

/**
 * Stateless tool executor
 */
export class StatelessExecutor extends BaseExecutor {
  private functionRegistry: FunctionRegistry;

  constructor(config: Partial<FunctionRegistryConfig> = {}) {
    super();
    this.functionRegistry = new FunctionRegistry(config);
  }

  /**
   * Specific implementation of executing a stateless tool
   * @param tool Tool definition
   * @param parameters Tool parameters
   * @param threadId Thread ID (optional; stateless tools do not use it)
   * @returns Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    _threadId?: string,
  ): Promise<unknown> {
    // Get the function to be executed
    const config = tool.config as StatelessToolConfig;
    if (!config || !config.execute) {
      throw new ToolError(
        `Tool '${tool.id}' does not have an execute function`,
        tool.id,
        "STATELESS",
        { hasConfig: !!config, hasExecute: !!config?.execute },
      );
    }

    if (typeof config.execute !== "function") {
      throw new ToolError(`Execute for tool '${tool.id}' is not a function`, tool.id, "STATELESS", {
        executeType: typeof config.execute,
      });
    }

    try {
      // Register the function (if it has not been registered yet)
      if (!this.functionRegistry.has(tool.id)) {
        this.functionRegistry.register(
          tool.id,
          config.execute as (parameters: Record<string, unknown>) => Promise<ToolOutput>,
          config.version,
          config.description,
        );
      }

      // Execute the function to obtain ToolOutput.
      const output = await this.functionRegistry.execute(tool.id, parameters);

      // If the tool execution fails, an error is thrown.
      if (!output.success) {
        throw new ToolError(output.error || "Tool execution failed", tool.id, "STATELESS", {
          parameters,
        });
      }

      // Returns the result, with ToolOutput.content as the result.
      return {
        result: output.content,
        functionStats: this.functionRegistry.getFunctionStats(tool.id),
      };
    } catch (error) {
      if (error instanceof ToolError) {
        throw error;
      }
      throw new ToolError(
        `Stateless tool execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        tool.id,
        "STATELESS",
        { parameters },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Register function
   */
  registerFunction(
    toolId: ID,
    execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>,
    version?: string,
    description?: string,
  ): void {
    this.functionRegistry.register(toolId, execute, version, description);
  }

  /**
   * Batch registration function
   */
  registerBatch(
    functions: Record<
      string,
      {
        execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>;
        version?: string;
        description?: string;
      }
    >,
  ): void {
    this.functionRegistry.registerBatch(functions);
  }

  /**
   * Logout function
   */
  unregisterFunction(toolId: ID): boolean {
    return this.functionRegistry.unregister(toolId);
  }

  /**
   * Obtain function information
   */
  getFunctionInfo(toolId: ID): unknown | null {
    return this.functionRegistry.get(toolId);
  }

  /**
   * Get all function information
   */
  getAllFunctionInfo(): Map<string, unknown> {
    return this.functionRegistry.getAllFunctionInfo();
  }

  /**
   * Obtain function statistics information
   */
  getFunctionStats(toolId: ID): unknown | null {
    return this.functionRegistry.getFunctionStats(toolId);
  }

  /**
   * Get all function names
   */
  getFunctionNames(): string[] {
    return this.functionRegistry.getFunctionNames();
  }

  /**
   * Get the number of registered functions.
   */
  getFunctionCount(): number {
    return this.functionRegistry.getFunctionCount();
  }

  /**
   * Clear the registry.
   */
  clearRegistry(): void {
    this.functionRegistry.clear();
  }

  /**
   * Obtain the executor type.
   */
  getExecutorType(): string {
    return "STATELESS";
  }
}
