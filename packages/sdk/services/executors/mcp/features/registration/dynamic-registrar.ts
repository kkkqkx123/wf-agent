/**
 * MCP Tools Dynamic Registrar
 *
 * Dynamically registers MCP tools as SDK Tool instances.
 * Supports the "mixed mode" approach of combining use_mcp tool with direct MCP tool registration.
 */

import type { Tool } from "@wf-agent/types";
import type { ToolRegistry } from "../../../../../shared/registry/tool-registry.js";
import type { McpConnectionManager } from "../../core/connection-manager.js";
import type { McpToolInfo } from "../metadata/tool-metadata-exporter.js";
import { McpToolMetadataExporter } from "../metadata/tool-metadata-exporter.js";
import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpToolsRegistrar" });

/**
 * Options for MCP tool registration
 */
export interface McpToolRegistrationOptions {
  /** Whether to register only hot tools */
  onlyHotTools?: boolean;
  /** Maximum number of tools to register */
  maxTools?: number;
  /** Tool name prefix (default: "mcp_") */
  toolNamePrefix?: string;
  /** Whether to auto-unregister tools when servers disconnect */
  autoUnregister?: boolean;
}

/**
 * MCP Tools Registrar
 *
 * Dynamically registers MCP tools as SDK Tool instances,
 * enabling direct LLM access to MCP server tools.
 */
export class McpToolsRegistrar {
  private registeredToolIds = new Set<string>();
  private registrationMap = new Map<string, { serverName: string; toolName: string }>();
  private options: Required<McpToolRegistrationOptions>;

  constructor(options?: McpToolRegistrationOptions) {
    this.options = {
      onlyHotTools: options?.onlyHotTools ?? false,
      maxTools: options?.maxTools ?? 20,
      toolNamePrefix: options?.toolNamePrefix ?? "mcp_",
      autoUnregister: options?.autoUnregister ?? true,
    };
  }

  /**
   * Register MCP tools as SDK tools
   *
   * @param toolRegistry - Tool registry to register tools into
   * @param mcpManager - MCP connection manager
   * @param options - Registration options (overrides constructor options)
   * @returns Array of registered tool IDs
   */
  async registerMcpTools(
    toolRegistry: ToolRegistry,
    mcpManager: McpConnectionManager,
    options?: Partial<McpToolRegistrationOptions>,
  ): Promise<string[]> {
    const mergedOptions = { ...this.options, ...options };
    const exporter = new McpToolMetadataExporter(mcpManager);

    // Get tools to register
    let toolsToRegister: McpToolInfo[];

    if (mergedOptions.onlyHotTools) {
      toolsToRegister = exporter.getHotTools(mergedOptions.maxTools);
    } else {
      toolsToRegister = exporter.exportAllTools().slice(0, mergedOptions.maxTools);
    }

    const registeredIds: string[] = [];

    for (const mcpTool of toolsToRegister) {
      try {
        const toolId = this.createToolId(mcpTool, mergedOptions.toolNamePrefix);

        // Skip if already registered
        if (this.registeredToolIds.has(toolId)) {
          logger.debug("Tool already registered, skipping", { toolId });
          continue;
        }

        // Create SDK tool from MCP tool
        const sdkTool = this.createSdkTool(toolId, mcpTool);

        // Register tool
        await toolRegistry.register(sdkTool);

        // Track registration
        this.registeredToolIds.add(toolId);
        this.registrationMap.set(toolId, {
          serverName: mcpTool.serverName,
          toolName: mcpTool.name,
        });

        registeredIds.push(toolId);

        logger.debug("MCP tool registered", {
          toolId,
          server: mcpTool.serverName,
          tool: mcpTool.name,
        });
      } catch (error) {
        logger.error("Failed to register MCP tool", {
          toolName: mcpTool.name,
          error,
        });
      }
    }

    logger.info("MCP tools registered", {
      count: registeredIds.length,
      total: toolsToRegister.length,
    });

    return registeredIds;
  }

  /**
   * Unregister MCP tools
   *
   * @param toolRegistry - Tool registry
   * @param toolIds - Tool IDs to unregister (if not provided, unregisters all)
   * @returns Array of unregistered tool IDs
   */
  async unregisterMcpTools(toolRegistry: ToolRegistry, toolIds?: string[]): Promise<string[]> {
    const toUnregister = toolIds || Array.from(this.registeredToolIds);
    const unregisteredIds: string[] = [];

    for (const toolId of toUnregister) {
      try {
        if (toolRegistry.delete(toolId)) {
          this.registeredToolIds.delete(toolId);
          this.registrationMap.delete(toolId);
          unregisteredIds.push(toolId);

          logger.debug("MCP tool unregistered", { toolId });
        }
      } catch (error) {
        logger.error("Failed to unregister MCP tool", { toolId, error });
      }
    }

    logger.info("MCP tools unregistered", { count: unregisteredIds.length });
    return unregisteredIds;
  }

  /**
   * Create tool ID from MCP tool info
   *
   * @param mcpTool - MCP tool info
   * @param prefix - Tool ID prefix
   * @returns Generated tool ID
   */
  private createToolId(mcpTool: McpToolInfo, prefix: string): string {
    // Convert server/tool names to valid identifier
    const serverPart = mcpTool.serverName.toLowerCase().replace(/[^a-z0-9]/g, "_");
    const toolPart = mcpTool.name.toLowerCase().replace(/[^a-z0-9]/g, "_");
    return `${prefix}${serverPart}__${toolPart}`;
  }

  /**
   * Create SDK tool from MCP tool info
   *
   * @param toolId - SDK tool ID
   * @param mcpTool - MCP tool info
   * @returns SDK tool definition
   */
  private createSdkTool(toolId: string, mcpTool: McpToolInfo): Tool {
    // Convert input schema to SDK parameter schema
    const properties = (mcpTool.inputSchema?.["properties"] as Record<string, unknown>) || {};
    const required = ((mcpTool.inputSchema?.["required"] as string[]) || []) as string[];

    const parameters = {
      type: "object" as const,
      properties,
      required,
    };

    const config: Record<string, unknown> = {
      serverName: mcpTool.serverName,
      toolName: mcpTool.name,
    };

    return {
      id: toolId,
      type: "MCP" as unknown as Tool['type'],
      description: `${mcpTool.description || "MCP tool"} (from server: ${mcpTool.serverName})`,
      parameters: parameters as Tool['parameters'],
      config: config as Tool['config'],
      metadata: {
        customFields: {
          source: "mcp_dynamic_registration",
          originalServerId: mcpTool.serverName,
          originalToolId: mcpTool.name,
        },
      },
    };
  }

  /**
   * Get registered tool IDs
   *
   * @returns Set of registered tool IDs
   */
  getRegisteredToolIds(): Set<string> {
    return new Set(this.registeredToolIds);
  }

  /**
   * Get MCP tool info for a registered tool
   *
   * @param toolId - SDK tool ID
   * @returns MCP tool info or undefined
   */
  getMcpToolInfo(toolId: string): { serverName: string; toolName: string } | undefined {
    return this.registrationMap.get(toolId);
  }

  /**
   * Check if a tool is registered
   *
   * @param toolId - Tool ID
   * @returns true if tool is registered
   */
  isToolRegistered(toolId: string): boolean {
    return this.registeredToolIds.has(toolId);
  }

  /**
   * Get statistics
   *
   * @returns Registration statistics
   */
  getStatistics(): {
    totalRegistered: number;
    registeredToolIds: string[];
  } {
    return {
      totalRegistered: this.registeredToolIds.size,
      registeredToolIds: Array.from(this.registeredToolIds),
    };
  }

  /**
   * Clear all registrations
   */
  clearRegistrations(): void {
    this.registeredToolIds.clear();
    this.registrationMap.clear();
    logger.debug("All MCP tool registrations cleared");
  }
}

/**
 * Create a default MCP tools registrar
 *
 * @param options - Registration options
 * @returns Configured registrar
 */
export function createMcpToolsRegistrar(options?: McpToolRegistrationOptions): McpToolsRegistrar {
  return new McpToolsRegistrar(options);
}
