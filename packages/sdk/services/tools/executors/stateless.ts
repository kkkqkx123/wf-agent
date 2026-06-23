/**
 * Stateless Tool Executor
 * Executes stateless functions provided by the application layer, manages functions through a function registry, and supports version control and call statistics.
 */

import type { Tool, ID, ToolOutput } from "@wf-agent/types";
import type { StatelessToolConfig } from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";
import { BaseExecutor } from "../core/base.js";

/**
 * Function registry entries
 */
export interface FunctionRegistryItem {
  /** function */
  execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>;
  /** Version */
  version?: string;
  /** Description */
  description?: string;
  /** Registration time */
  registeredAt: Date;
  /** Call count */
  callCount: number;
  /** Last call time */
  lastCalledAt?: Date;
}

/**
 * Function registry configuration
 */
export interface FunctionRegistryConfig {
  /** Whether to enable version control */
  enableVersionControl: boolean;
  /** Whether to record call statistics */
  enableCallStatistics: boolean;
  /** Maximum number of registered functions */
  maxFunctions: number;
}

/**
 * Stateless tool executor
 */
export class StatelessExecutor extends BaseExecutor {
  private functions: Map<string, FunctionRegistryItem> = new Map();
  private config: FunctionRegistryConfig;

  constructor(config: Partial<FunctionRegistryConfig> = {}) {
    super();
    this.config = {
      enableVersionControl: config.enableVersionControl ?? true,
      enableCallStatistics: config.enableCallStatistics ?? true,
      maxFunctions: config.maxFunctions ?? 100,
    };
  }

  /**
   * Specific implementation of executing a stateless tool
   * @param tool Tool definition
   * @param parameters Tool parameters
   * @param executionId Execution ID (optional; stateless tools do not use it)
   * @param context Execution context (optional, for interactive tools)
   * @returns Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    _executionId?: string,
    _context?: Record<string, unknown>,
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
      if (!this.functions.has(tool.id)) {
        this.register(
          tool.id,
          config.execute as (parameters: Record<string, unknown>) => Promise<ToolOutput>,
          config.version,
          config.description,
        );
      }

      // Execute the function to obtain ToolOutput.
      const output = await this.executeFunction(tool.id, parameters);

      // If the tool execution fails, an error is thrown.
      if (!output.success) {
        throw new ToolError(output.error || "Tool execution failed", tool.id, "STATELESS", {
          parameters,
        });
      }

      // Returns the result, with ToolOutput.content as the result.
      return {
        result: output.content,
        functionStats: this.getFunctionStats(tool.id),
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
  private register(
    toolId: ID,
    execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>,
    version?: string,
    description?: string,
  ): void {
    if (this.functions.size >= this.config.maxFunctions) {
      throw new ToolError(
        `Maximum functions (${this.config.maxFunctions}) reached`,
        toolId,
        "STATELESS",
        { currentFunctions: this.functions.size },
      );
    }

    if (this.functions.has(toolId)) {
      if (this.config.enableVersionControl) {
        // eslint-disable-next-line no-console
        console.warn(`Function '${toolId}' already registered, overwriting...`);
      } else {
        throw new ToolError(`Function '${toolId}' is already registered`, toolId, "STATELESS", {
          toolId,
        });
      }
    }

    const item: FunctionRegistryItem = {
      execute,
      version,
      description,
      registeredAt: new Date(),
      callCount: 0,
    };

    this.functions.set(toolId, item);
  }

  /**
   * Get the function
   */
  private getFunction(toolId: ID): FunctionRegistryItem | null {
    return this.functions.get(toolId) || null;
  }

  /**
   * Execute the function
   */
  private executeFunction(toolId: ID, parameters: Record<string, unknown>): Promise<ToolOutput> {
    const item = this.getFunction(toolId);

    if (!item) {
      throw new ToolError(`Function '${toolId}' is not registered`, toolId, "STATELESS", {
        toolId,
      });
    }

    return Promise.resolve()
      .then(async () => {
        const result = await item.execute(parameters);

        if (this.config.enableCallStatistics) {
          item.callCount++;
          item.lastCalledAt = new Date();
        }

        return result;
      })
      .catch(error => {
        throw new ToolError(
          `Function execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
          toolId,
          "STATELESS",
          { parameters },
          error instanceof Error ? error : undefined,
        );
      });
  }

  /**
   * Get function statistics information
   */
  private getFunctionStats(toolId: ID): {
    callCount: number;
    lastCalledAt?: Date;
    registeredAt: Date;
  } | null {
    const item = this.functions.get(toolId);
    if (!item) {
      return null;
    }

    return {
      callCount: item.callCount,
      lastCalledAt: item.lastCalledAt,
      registeredAt: item.registeredAt,
    };
  }

  /**
   * Obtain the executor type.
   */
  getExecutorType(): string {
    return "STATELESS";
  }
}
