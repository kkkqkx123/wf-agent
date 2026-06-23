/**
 * MCP Tool Metadata Exporter
 *
 * Exports MCP tool metadata for dynamic context injection, tool discovery,
 * and LLM prompt enhancement. Follows the dynamic-context pattern defined
 * in packages/types/src/dynamic-context.ts
 */

import type { McpTool } from "../../types.js";
import type { McpConnectionManager } from "../../core/connection-manager.js";
import type { McpServerState } from "../../types.js";
import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpToolMetadataExporter" });

/**
 * MCP Tool metadata including server context
 */
export interface McpToolInfo extends McpTool {
  /** Server name that hosts this tool */
  serverName: string;
  /** Full tool identifier: serverName/toolName */
  fullId: string;
}

/**
 * MCP Server metadata
 */
export interface McpServerMetadata {
  /** Server name */
  name: string;
  /** Server connection status */
  status: "connected" | "disconnected" | "connecting";
  /** Server-level instructions */
  instructions?: string;
  /** Available tools */
  tools: McpToolInfo[];
  /** Number of available resources */
  resourceCount: number;
}

/**
 * Exported MCP tools context for LLM prompt injection
 */
export interface ExportedMcpToolsContext {
  /** Whether MCP is available */
  hasServers: boolean;
  /** Total number of servers */
  serverCount: number;
  /** Total number of tools */
  toolCount: number;
  /** List of servers and their tools */
  servers: McpServerMetadata[];
  /** Top tools by frequency (for hot tool promotion) */
  hotTools: McpToolInfo[];
  /** Plain text summary for LLM consumption */
  summary: string;
}

/**
 * MCP Tool Metadata Exporter
 *
 * Extracts and formats MCP tool metadata for integration with dynamic context.
 * Designed to be called by TransformContextFn to enhance LLM prompts with
 * information about available MCP tools and servers.
 */
export class McpToolMetadataExporter {
  constructor(private connectionManager: McpConnectionManager) {}

  /**
   * Export all available MCP tools from connected servers
   *
   * @returns Array of tools with server context
   */
  exportAllTools(): McpToolInfo[] {
    const allTools: McpToolInfo[] = [];
    const servers = this.connectionManager.getAllServerStates();

    for (const server of servers) {
      if (!server.tools) continue;

      for (const tool of server.tools) {
        allTools.push({
          ...tool,
          serverName: server.name,
          fullId: `${server.name}/${tool.name}`,
        });
      }
    }

    logger.debug("Exported tools", { count: allTools.length, servers: servers.length });
    return allTools;
  }

  /**
   * Export metadata for a specific server
   *
   * @param serverName - Server name
   * @returns Server metadata or undefined if not found
   */
  exportServerMetadata(serverName: string): McpServerMetadata | undefined {
    const state = this.connectionManager.getServerState(serverName);
    if (!state) return undefined;

    const tools: McpToolInfo[] = (state.tools || []).map((tool: McpTool) => ({
      ...tool,
      serverName: state.name,
      fullId: `${state.name}/${tool.name}`,
    }));

    return {
      name: state.name,
      status: state.status,
      instructions: state.instructions,
      tools,
      resourceCount: state.resources?.length ?? 0,
    };
  }

  /**
   * Export metadata for all servers with available tools/resources
   *
   * @returns Array of server metadata
   */
  exportAllServersMetadata(): McpServerMetadata[] {
    const servers = this.connectionManager.getAllServerStates();
    return servers
      .map((state: McpServerState) => ({
        name: state.name,
        status: state.status,
        instructions: state.instructions,
        tools: (state.tools || []).map((tool: McpTool) => ({
          ...tool,
          serverName: state.name,
          fullId: `${state.name}/${tool.name}`,
        })),
        resourceCount: state.resources?.length ?? 0,
      }))
      .filter((server: McpServerMetadata) => server.tools.length > 0 || server.resourceCount > 0);
  }

  /**
   * Get top N tools (for hot tool promotion in prompts)
   *
   * @param limit - Maximum number of tools to return
   * @returns Top tools
   */
  getHotTools(limit: number = 10): McpToolInfo[] {
    const allTools = this.exportAllTools();
    // Sort by enabledForPrompt first, then by name for consistency
    return allTools
      .sort((a: McpToolInfo, b: McpToolInfo) => {
        if (a.enabledForPrompt !== b.enabledForPrompt) {
          return (b.enabledForPrompt ? 1 : 0) - (a.enabledForPrompt ? 1 : 0);
        }
        return a.name.localeCompare(b.name);
      })
      .slice(0, limit);
  }

  /**
   * Generate plain text summary of available MCP tools for LLM consumption
   *
   * @param options - Export options
   * @returns Formatted text summary
   */
  generateToolsSummary(options?: { includeResourceCount?: boolean; hotToolsLimit?: number }): string {
    const servers = this.exportAllServersMetadata();

    if (servers.length === 0) {
      return "No MCP servers configured.";
    }

    const hotTools = this.getHotTools(options?.hotToolsLimit ?? 5);
    const lines: string[] = [];

    lines.push(`# Available MCP Tools (${servers.length} server${servers.length !== 1 ? "s" : ""})`);
    lines.push("");

    // List all servers and their tools
    for (const server of servers) {
      lines.push(`## ${server.name}`);
      if (server.instructions) {
        lines.push(`**Instructions:** ${server.instructions}`);
      }
      if (server.status !== "connected") {
        lines.push(`**Status:** ${server.status}`);
      }

      if (server.tools.length > 0) {
        lines.push(`**Tools (${server.tools.length}):**`);
        for (const tool of server.tools.slice(0, 10)) {
          const desc = tool.description ? ` - ${tool.description}` : "";
          lines.push(`- \`${tool.name}\`${desc}`);
        }
        if (server.tools.length > 10) {
          lines.push(`- ... and ${server.tools.length - 10} more`);
        }
      }

      if (options?.includeResourceCount && server.resourceCount > 0) {
        lines.push(`**Resources:** ${server.resourceCount} available`);
      }

      lines.push("");
    }

    // Add hot tools section if available
    if (hotTools.length > 0) {
      lines.push("## Hot Tools (Recommended)");
      for (const tool of hotTools) {
        const desc = tool.description ? ` - ${tool.description}` : "";
        lines.push(`- \`${tool.fullId}\`${desc}`);
      }
      lines.push("");
    }

    lines.push("## Usage");
    lines.push("Call any tool using: use_mcp(server=<server_name>, tool=<tool_name>, arguments=<args>)");

    return lines.join("\n");
  }

  /**
   * Export complete MCP tools context for dynamic context injection
   *
   * This method generates the full context structure that can be used in
   * a TransformContextFn to enhance LLM prompts with MCP tool information.
   *
   * @param options - Export options
   * @returns Complete exported context
   */
  exportContext(options?: {
    includeResourceCount?: boolean;
    hotToolsLimit?: number;
  }): ExportedMcpToolsContext {
    const servers = this.exportAllServersMetadata();
    const allTools = this.exportAllTools();
    const hotTools = this.getHotTools(options?.hotToolsLimit ?? 5);

    const context: ExportedMcpToolsContext = {
      hasServers: servers.length > 0,
      serverCount: servers.length,
      toolCount: allTools.length,
      servers,
      hotTools,
      summary: this.generateToolsSummary({
        includeResourceCount: options?.includeResourceCount,
        hotToolsLimit: options?.hotToolsLimit,
      }),
    };

    logger.debug("Exported MCP context", {
      servers: context.serverCount,
      tools: context.toolCount,
      hotTools: context.hotTools.length,
    });

    return context;
  }
}
