/**
 * Builtin Tool Executor
 * Executes built-in SDK tools like execute_workflow
 */

import type { Tool, BuiltinToolConfig, BuiltinToolExecutionContext } from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";
import { BaseExecutor } from "../core/base/BaseExecutor.js";
import type { BuiltinExecutorConfig } from "./types.js";

/**
 * Builtin tool executor
 */
export class BuiltinExecutor extends BaseExecutor {
  private defaultContext: Partial<BuiltinToolExecutionContext>;

  constructor(config: BuiltinExecutorConfig = {}) {
    super();
    this.defaultContext = config.defaultContext ?? {};
  }

  /**
   * Specific implementation of executing a builtin tool
   * @param tool Tool definition
   * @param parameters Tool parameters
   * @param executionId Execution ID (optional)
   * @returns Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
  ): Promise<unknown> {
    // Get the builtin tool config
    const config = tool.config as BuiltinToolConfig;
    if (!config || !config.execute) {
      throw new ToolError(
        `Tool '${tool.id}' does not have a valid builtin config`,
        tool.id,
        "BUILTIN",
        { hasConfig: !!config, hasExecute: !!config?.execute },
      );
    }

    if (typeof config.execute !== "function") {
      throw new ToolError(`Execute for tool '${tool.id}' is not a function`, tool.id, "BUILTIN", {
        executeType: typeof config.execute,
      });
    }

    try {
      // Build execution context
      const context: BuiltinToolExecutionContext = {
        executionId,
        ...this.defaultContext,
      };

      // Execute the builtin tool
      const result = await config.execute(parameters, context);

      return {
        result,
        toolName: config.name,
      };
    } catch (error) {
      if (error instanceof ToolError) {
        throw error;
      }
      throw new ToolError(
        `Builtin tool execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        tool.id,
        "BUILTIN",
        { parameters },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Update the default execution context
   * @param context Partial context to merge
   */
  updateDefaultContext(context: Partial<BuiltinToolExecutionContext>): void {
    this.defaultContext = { ...this.defaultContext, ...context };
  }

  /**
   * Get the executor type
   */
  getExecutorType(): string {
    return "BUILTIN";
  }
}
