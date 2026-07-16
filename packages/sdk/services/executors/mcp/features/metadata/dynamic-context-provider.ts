/**
 * MCP Tools Dynamic Context Provider
 *
 * Provides dynamic context about available MCP tools for injection into LLM prompts.
 * Follows the ephemeral dynamic context pattern from packages/types/src/dynamic-context.ts
 *
 * This module integrates with the TransformContextFn mechanism to enhance LLM prompts
 * with information about available MCP tools and servers at runtime.
 *
 * Design: Minimizes prompt noise by:
 * - Only showing essential tool information
 * - Limiting tool listings per server
 * - Avoiding duplicate information
 * - Making all content customizable
 */

import type { LLMMessage, TransformContextFn, DynamicPromptInjection } from "@wf-agent/types";
import type { McpConnectionManager } from "../../core/connection-manager.js";
import { McpToolMetadataExporter } from "./tool-metadata-exporter.js";
import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpToolsDynamicContextProvider" });

/**
 * Options for MCP tools context generation
 * All options default to minimal/no content to avoid prompt pollution
 */
export interface McpToolsContextOptions {
  /** Whether to include MCP tools in the context (default: true) */
  enabled?: boolean;

  /** Maximum number of tools to show per server (default: 5, 0 = show all) */
  toolsPerServer?: number;

  /** Maximum number of hot tools to include (default: 0 = disabled, set to >0 to enable) */
  hotToolsLimit?: number;

  /** Whether to include server connection status (default: false) */
  includeServerStatus?: boolean;

  /** Whether to include resource counts in the summary (default: false) */
  includeResourceCount?: boolean;

  /** Whether to include tool descriptions (default: true) */
  includeToolDescriptions?: boolean;

  /** Whether to include server-level instructions (default: false) */
  includeServerInstructions?: boolean;

  /** Position to inject context: 'before' (default) or 'after' last message */
  injectionPosition?: "before" | "after";

  /** Custom context prefix/header (default: "# Available MCP Tools") */
  contextPrefix?: string;

  /** Include usage hint/guide (default: true) */
  includeUsageHint?: boolean;

  /**
   * Compact mode: only list tool names without descriptions (default: false)
   * Overrides includeToolDescriptions if true
   */
  compactMode?: boolean;
}

/**
 * Generated context about MCP tools
 */
export interface GeneratedMcpToolsContext {
  /** Whether any MCP servers are configured */
  hasServers: boolean;
  /** The formatted context message for injection */
  content: string;
  /** Number of servers available */
  serverCount: number;
  /** Number of tools available */
  toolCount: number;
  /** Approximate content length for decision-making */
  contentLength: number;
}

/**
 * MCP Tools Dynamic Context Provider
 *
 * Generates and manages dynamic context about available MCP tools for LLM prompts.
 * Uses McpConnectionManager to access server information and McpToolMetadataExporter
 * to format tool metadata for LLM consumption.
 *
 * Designed to minimize prompt noise by providing fine-grained control over what information
 * is included in the context.
 */
export class McpToolsDynamicContextProvider {
  private exporter: McpToolMetadataExporter;

  constructor(mcpManager: McpConnectionManager) {
    this.exporter = new McpToolMetadataExporter(mcpManager);
  }

  /**
   * Generate context about available MCP tools
   *
   * @param options - Context generation options (all default to minimal content)
   * @returns Generated context information
   */
  generateContext(options?: McpToolsContextOptions): GeneratedMcpToolsContext {
    const config: Required<McpToolsContextOptions> = {
      enabled: options?.enabled ?? true,
      toolsPerServer: options?.toolsPerServer ?? 5,
      hotToolsLimit: options?.hotToolsLimit ?? 0,
      includeServerStatus: options?.includeServerStatus ?? false,
      includeResourceCount: options?.includeResourceCount ?? false,
      includeToolDescriptions: options?.includeToolDescriptions ?? true,
      includeServerInstructions: options?.includeServerInstructions ?? false,
      injectionPosition: options?.injectionPosition ?? "before",
      contextPrefix: options?.contextPrefix ?? "# Available MCP Tools",
      includeUsageHint: options?.includeUsageHint ?? true,
      compactMode: options?.compactMode ?? false,
    };

    if (!config.enabled) {
      return {
        hasServers: false,
        content: "",
        serverCount: 0,
        toolCount: 0,
        contentLength: 0,
      };
    }

    const context = this.exporter.exportContext({
      hotToolsLimit: config.hotToolsLimit,
      includeResourceCount: config.includeResourceCount,
    });

    if (!context.hasServers) {
      return {
        hasServers: false,
        content: "No MCP servers are currently configured.",
        serverCount: 0,
        toolCount: 0,
        contentLength: 45,
      };
    }

    // Build minimal but useful context message
    const lines: string[] = [];
    lines.push(config.contextPrefix);
    lines.push("");

    // Brief status line
    lines.push(`${context.serverCount} server(s) with ${context.toolCount} tool(s) available`);
    lines.push("");

    // Add server list with tools
    for (const server of context.servers) {
      lines.push(`## ${server.name}`);

      // Include server instructions only if requested
      if (config.includeServerInstructions && server.instructions) {
        lines.push(`*${server.instructions}*`);
      }

      // Include status only if explicitly requested
      if (config.includeServerStatus && server.status !== "connected") {
        lines.push(`⚠️ Status: ${server.status}`);
      }

      if (server.tools.length > 0) {
        // Limit tools per server to avoid noise
        const toolsToShow = config.toolsPerServer > 0
          ? server.tools.slice(0, config.toolsPerServer)
          : server.tools;

        if (config.compactMode) {
          // Compact mode: just list tool names
          const toolNames = toolsToShow.map(t => `\`${t.name}\``).join(", ");
          lines.push(toolNames);
        } else {
          // Normal mode: tool names with optional descriptions
          for (const tool of toolsToShow) {
            if (config.includeToolDescriptions && tool.description) {
              lines.push(`- \`${tool.name}\` — ${tool.description}`);
            } else {
              lines.push(`- \`${tool.name}\``);
            }
          }
        }

        if (server.tools.length > toolsToShow.length) {
          lines.push(`*... and ${server.tools.length - toolsToShow.length} more*`);
        }
      }

      if (config.includeResourceCount && server.resourceCount > 0) {
        lines.push(`Resources: ${server.resourceCount} available`);
      }

      lines.push("");
    }

    // Add hot tools section ONLY if explicitly enabled
    if (config.hotToolsLimit > 0 && context.hotTools.length > 0) {
      lines.push("## Recommended Tools");
      const hotTools = context.hotTools.slice(0, config.hotToolsLimit);
      for (const tool of hotTools) {
        const desc = config.includeToolDescriptions && tool.description
          ? ` — ${tool.description}`
          : "";
        lines.push(`- \`${tool.fullId}\`${desc}`);
      }
      lines.push("");
    }

    // Add usage guide only if explicitly enabled
    if (config.includeUsageHint) {
      lines.push("Use: `use_mcp(server_name=\"...\", tool_name=\"...\")`");
    }

    const content = lines.join("\n");

    logger.debug("Generated MCP tools context", {
      hasServers: context.hasServers,
      servers: context.serverCount,
      tools: context.toolCount,
      contentLength: content.length,
      compactMode: config.compactMode,
    });

    return {
      hasServers: context.hasServers,
      content,
      serverCount: context.serverCount,
      toolCount: context.toolCount,
      contentLength: content.length,
    };
  }

  /**
   * Create a TransformContextFn for use with AgentLoopRuntimeConfig
   *
   * This function can be used as the transformContext callback to automatically
   * inject MCP tools context into LLM prompts.
   *
   * @deprecated Use {@link createDynamicPromptInjectionFn} instead, which integrates
   * with the standard TransformContextFn pipeline (AgentLoopRuntimeConfig.transformContext).
   * The new API returns `DynamicPromptInjection` and is compatible with the core
   * dynamic injection system via `injectDynamicPrompts`.
   *
   * @param options - Context generation options
   * @returns TransformContextFn implementation
   */
  createTransformContextFn(options?: McpToolsContextOptions) {
    return async (messages: LLMMessage[]): Promise<LLMMessage[]> => {
      const context = this.generateContext(options);

      // Don't inject if no servers
      if (!context.hasServers) {
        logger.debug("Skipping MCP context injection: no servers configured");
        return messages;
      }

      // Create context message
      const contextMessage: LLMMessage = {
        role: "user",
        content: context.content,
      };

      // Inject context message based on position
      const injectionPosition = options?.injectionPosition ?? "before";
      if (injectionPosition === "after") {
        // Add at the end
        return [...messages, contextMessage];
      } else {
        // Add at the beginning (after system prompt if present)
        const firstMsg = messages[0];
        if (firstMsg?.role === "system") {
          return [firstMsg, contextMessage, ...messages.slice(1)];
        }
        return [contextMessage, ...messages];
      }
    };
  }

  /**
   * Create a TransformContextFn-compatible injection function
   *
   * Returns a function compatible with AgentLoopRuntimeConfig.transformContext,
   * allowing MCP context to be injected through the standard dynamic injection pipeline.
   * The MCP context content is returned as staticSystem (stable across iterations).
   *
   * @param options - Context options
   * @returns TransformContextFn that returns DynamicPromptInjection
   */
  createDynamicPromptInjectionFn(options?: McpToolsContextOptions): TransformContextFn {
    return async (_context): Promise<DynamicPromptInjection> => {
      const mcpContext = this.generateContext(options);

      if (!mcpContext.hasServers) {
        return { staticSystem: undefined, dynamicUserContext: undefined };
      }

      return {
        staticSystem: mcpContext.content,
        dynamicUserContext: undefined,
      };
    };
  }

  /**
   * Inject MCP tools context into message array
   *
   * Utility method for manual context injection.
   *
   * @param messages - Original messages
   * @param options - Context options
   * @returns Messages with injected context
   */
  injectContext(messages: LLMMessage[], options?: McpToolsContextOptions): LLMMessage[] {
    const context = this.generateContext(options);

    if (!context.hasServers) {
      return messages;
    }

    const contextMessage: LLMMessage = {
      role: "user",
      content: context.content,
    };

    const injectionPosition = options?.injectionPosition ?? "before";
    if (injectionPosition === "after") {
      return [...messages, contextMessage];
    } else {
      const firstMsg = messages[0];
      if (firstMsg?.role === "system") {
        return [firstMsg, contextMessage, ...messages.slice(1)];
      }
      return [contextMessage, ...messages];
    }
  }

  /**
   * Get statistics about MCP tools context
   *
   * @returns Context statistics
   */
  getContextStats(): {
    hasMcpServers: boolean;
    serverCount: number;
    toolCount: number;
    contentPreview: string;
  } {
    const context = this.generateContext();
    return {
      hasMcpServers: context.hasServers,
      serverCount: context.serverCount,
      toolCount: context.toolCount,
      contentPreview: context.content.split("\n").slice(0, 5).join("\n") + "...",
    };
  }
}

/**
 * Create a default MCP tools context provider
 *
 * Convenience factory for creating a provider with default configuration.
 *
 * @param mcpManager - MCP connection manager
 * @returns Configured provider
 */
export function createMcpToolsContextProvider(
  mcpManager: McpConnectionManager,
): McpToolsDynamicContextProvider {
  return new McpToolsDynamicContextProvider(mcpManager);
}
