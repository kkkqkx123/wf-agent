/**
 * MCP Tool Executor
 *
 * Executes MCP tools registered as SDK Tool with type "MCP".
 * Provides a wrapper around MCP server tool calls with error handling and context support.
 */

import type { Tool } from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";
import { BaseExecutor } from "../core/base.js";
import { McpServerRegistry } from "../../../services/executors/mcp/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpExecutor" });

/**
 * MCP Tool Configuration
 *
 * Stored in tool.config for MCP-type tools
 */
export interface McpToolConfig {
  /** MCP Server name */
  serverName: string;
  /** Tool name on the MCP server */
  toolName: string;
  /** Optional: timeout in milliseconds (overrides server timeout) */
  timeout?: number;
  /** Optional: whether to create checkpoints on success */
  createCheckpointOnSuccess?: boolean;
  /** Optional: checkpoint description template */
  checkpointDescriptionTemplate?: string;
}

/**
 * MCP Tool Executor
 *
 * Executes tools hosted on MCP servers. Handles tool invocation,
 * error processing, and context management.
 */
export class McpExecutor extends BaseExecutor {
  /**
   * Get executor type
   */
  override getExecutorType(): string {
    return "MCP";
  }

  /**
   * Execute an MCP tool (protected method required by BaseExecutor)
   *
   * @param tool - Tool definition with MCP config
   * @param parameters - Tool parameters
   * @param executionId - Execution ID
   * @param _context - Execution context (unused for MCP tools)
   * @returns Execution result
   */
  protected override async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
    _context?: Record<string, unknown>,
  ): Promise<unknown> {
    // Extract MCP configuration
    const config = tool.config as McpToolConfig;

    if (!config || !config.serverName || !config.toolName) {
      throw new ToolError(
        "Invalid MCP tool configuration: missing serverName or toolName",
        tool.id,
        tool.type,
        { parameters },
      );
    }

    // Get MCP manager instance
    const manager = await McpServerRegistry.getInstance();

    logger.debug("Executing MCP tool", {
      toolId: tool.id,
      server: config.serverName,
      tool: config.toolName,
    });

    // Execute tool
    const result = await manager.callTool(
      config.serverName,
      config.toolName,
      parameters,
    );

    // Process result content
    const outputText = this.processToolResult(result);

    if (!result.isError) {
      logger.debug("MCP tool executed successfully", {
        toolId: tool.id,
        outputLength: outputText.length,
      });
    } else {
      logger.warn("MCP tool execution resulted in error", {
        toolId: tool.id,
        output: outputText,
      });
    }

    return {
      success: !result.isError,
      content: outputText || "(No output)",
      metadata: {
        executionId,
        toolId: tool.id,
        serverName: config.serverName,
      },
    };
  }

  /**
   * Process MCP tool result content
   *
   * @param result - MCP tool call result
   * @returns Formatted output text
   */
  private processToolResult(result: {
    content: Array<{
      type: string;
      text?: string;
      data?: string;
      mimeType?: string;
    }>;
    isError?: boolean;
  }): string {
    const textContent = result.content
      .map(item => {
        if (item.type === "text") {
          return item.text || "";
        }
        if (item.type === "image" && item.mimeType && item.data) {
          const imageData = item.data.startsWith("data:")
            ? item.data
            : `data:${item.mimeType};base64,${item.data}`;
          return `[Image: ${imageData.substring(0, 100)}...]`;
        }
        if (item.type === "resource") {
          return JSON.stringify(item, null, 2);
        }
        return "";
      })
      .filter(Boolean)
      .join("\n\n");

    const prefix = result.isError ? "Error:\n" : "";
    return prefix + textContent;
  }
}
