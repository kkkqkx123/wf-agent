/**
 * The logic executed by the use_mcp tool
 *
 * This tool uses MCP server capabilities.
 * The handler returns the MCP execution result.
 */

import type { ToolOutput } from "@wf-agent/types";
import type { McpConnectionManager } from "../../../../../../services/mcp/index.js";

/**
 * Process MCP tool result content
 */
function processToolResult(result: {
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
        return `[Image: ${imageData.substring(0, 50)}...]`;
      }
      if (item.type === "resource") {
        return JSON.stringify(item, null, 2);
      }
      return "";
    })
    .filter(Boolean)
    .join("\n\n");

  return textContent;
}

/**
 * Process MCP resource result content
 */
function processResourceResult(result: {
  contents: Array<{
    uri: string;
    mimeType?: string;
    text?: string;
    blob?: string;
  }>;
}): string {
  const textContent =
    result.contents
      .map(item => {
        if (item.text) {
          return item.text;
        }
        if (item.mimeType?.startsWith("image") && item.blob) {
          const imageData = item.blob.startsWith("data:")
            ? item.blob
            : `data:${item.mimeType};base64,${item.blob}`;
          return `[Image: ${imageData.substring(0, 50)}...]`;
        }
        return "";
      })
      .filter(Boolean)
      .join("\n\n") || "(Empty response)";

  return textContent;
}

/**
 * Create the `use_mcp` tool execution function
 *
 * @param mcpManager - MCP connection manager instance
 */
export function createUseMcpHandler(mcpManager?: McpConnectionManager) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        server_name,
        tool_name,
        arguments: args,
        uri,
      } = params as {
        server_name: string;
        tool_name?: string;
        arguments?: Record<string, unknown>;
        uri?: string;
      };

      if (!server_name || typeof server_name !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'server_name' parameter",
        };
      }

      // If no MCP manager provided, return placeholder
      if (!mcpManager) {
        if (tool_name) {
          return {
            success: true,
            content: `[MCP not initialized] Tool call: ${server_name}.${tool_name}`,
          };
        } else if (uri) {
          return {
            success: true,
            content: `[MCP not initialized] Resource access: ${server_name}:${uri}`,
          };
        }
        return {
          success: false,
          content: "",
          error: "Must provide either 'tool_name' (for tool calls) or 'uri' (for resource access)",
        };
      }

      // Check if MCP is enabled
      if (!mcpManager.isMcpEnabled()) {
        return {
          success: false,
          content: "",
          error: "MCP is disabled. Enable MCP to use this tool.",
        };
      }

      // Get server state
      const serverState = mcpManager.getServerState(server_name);
      if (!serverState) {
        return {
          success: false,
          content: "",
          error: `MCP server "${server_name}" not found. Check server configuration.`,
        };
      }

      if (serverState.disabled) {
        return {
          success: false,
          content: "",
          error: `MCP server "${server_name}" is disabled.`,
        };
      }

      if (serverState.status !== "connected") {
        return {
          success: false,
          content: "",
          error: `MCP server "${server_name}" is not connected (status: ${serverState.status}).`,
        };
      }

      // Execute operation
      if (tool_name) {
        // Tool call
        const result = await mcpManager.callTool(server_name, tool_name, args);
        const text = processToolResult(result);

        const outputPrefix = result.isError ? "Error:\n" : "";
        const outputText = text || "(No output)";

        return {
          success: !result.isError,
          content: outputPrefix + outputText,
        };
      } else if (uri) {
        // Resource access
        const result = await mcpManager.readResource(server_name, uri);
        const text = processResourceResult(result);

        return {
          success: true,
          content: text,
        };
      } else {
        return {
          success: false,
          content: "",
          error: "Must provide either 'tool_name' (for tool calls) or 'uri' (for resource access)",
        };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        content: "",
        error: `MCP operation failed: ${errorMessage}`,
      };
    }
  };
}

/**
 * Create a handler with lazy MCP manager initialization
 * This is useful when the MCP manager is not available at handler creation time
 */
export function createLazyUseMcpHandler(
  getMcpManager: () => Promise<McpConnectionManager | undefined>,
) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    const mcpManager = await getMcpManager();
    const handler = createUseMcpHandler(mcpManager);
    return handler(params);
  };
}
