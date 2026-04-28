/**
 * Tool Executor Interface
 * Defines the core contracts that all executors must implement
 */

import type { Tool, ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";

/**
 * Tool Executor Interface
 */
export interface IToolExecutor {
  /**
   * Execution Tool
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @param options: Execution options
   * @param threadId: Thread ID (optional, used for thread isolation in stateful tools)
   * @returns: Execution result
   */
  execute(
    tool: Tool,
    parameters: Record<string, unknown>,
    options?: ToolExecutionOptions,
    threadId?: string,
  ): Promise<ToolExecutionResult>;

  /**
   * Clean up resources
   * @returns Promise
   */
  cleanup?(): Promise<void>;

  /**
   * Obtain the executor type.
   */
  getExecutorType(): string;
}
