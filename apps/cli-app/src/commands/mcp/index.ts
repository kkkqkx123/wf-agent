/**
 * MCP Command Group
 * Management commands for MCP server connections and diagnostics
 */

import { Command } from "commander";
import { McpAdapter } from "../../adapters/mcp-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create MCP Command Group
 */
export function createMcpCommands(): Command {
  const mcpCmd = new Command("mcp").description("Manage MCP (Model Context Protocol) servers");

  // --- mcp list ---
  mcpCmd
    .command("list")
    .description("List all registered MCP servers")
    .option("-c, --connected", "Show only connected servers")
    .option("-t, --table", "Output in tabular format")
    .action(async (options: CommandOptions & { connected?: boolean }) => {
      try {
        const adapter = new McpAdapter();
        const servers = options.connected
          ? await adapter.getConnectedServers()
          : await adapter.listServers();

        router.render(servers, {
          type: "list",
          entity: "mcp-server",
          format: () =>
            servers
              .map(
                (s) =>
                  `[${s.status || "unknown"}] ${s.name}` +
                  (s.disabled ? " (disabled)" : "") +
                  (s.errorHistory?.length ? ` (errors: ${s.errorHistory.length})` : ""),
              )
              .join("\n"),
          metadata: { total: servers.length },
        });
      } catch (error) {
        handleError(error, { operation: "list-mcp-servers" });
      }
    });

  // --- mcp show <name> ---
  mcpCmd
    .command("show <name>")
    .description("Show details of an MCP server")
    .action(async (name: string) => {
      try {
        const adapter = new McpAdapter();
        const server = await adapter.getServer(name);

        if (!server) {
          output.warnLog(`MCP server "${name}" not found`);
          return;
        }

        router.render(server, {
          type: "detail",
          entity: "mcp-server",
          format: () => JSON.stringify(server, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "show-mcp-server", additionalInfo: { name } });
      }
    });

  // --- mcp connect <name> ---
  mcpCmd
    .command("connect <name>")
    .description("Connect to an MCP server")
    .option("-c, --config <json>", "Server configuration as JSON string")
    .action(async (name: string, options: { config?: string }) => {
      try {
        let config: Record<string, unknown> | undefined;
        if (options.config) {
          try {
            config = JSON.parse(options.config);
          } catch {
            output.warnLog("Failed to parse --config as JSON, connecting without explicit config");
          }
        }

        const adapter = new McpAdapter();
        await adapter.connectServer(name, config);
        output.success(`Connected to MCP server "${name}"`);
      } catch (error) {
        handleError(error, { operation: "connect-mcp-server", additionalInfo: { name } });
      }
    });

  // --- mcp disconnect <name> ---
  mcpCmd
    .command("disconnect <name>")
    .description("Disconnect from an MCP server")
    .action(async (name: string) => {
      try {
        const adapter = new McpAdapter();
        await adapter.disconnectServer(name);
        output.success(`Disconnected from MCP server "${name}"`);
      } catch (error) {
        handleError(error, { operation: "disconnect-mcp-server", additionalInfo: { name } });
      }
    });

  // --- mcp status <name> ---
  mcpCmd
    .command("status <name>")
    .description("Show connection status of an MCP server")
    .action(async (name: string) => {
      try {
        const adapter = new McpAdapter();
        const state = await adapter.getServerStatus(name);

        if (!state) {
          output.warnLog(`MCP server "${name}" not found`);
          return;
        }

        router.render(state, {
          type: "detail",
          entity: "mcp-server-status",
          format: () =>
            [
              `Server: ${state.name}`,
              `Status: ${state.status || "unknown"}`,
              `Disabled: ${state.disabled ? "yes" : "no"}`,
              `Tools: ${state.tools?.length ?? 0}`,
              `Resources: ${state.resources?.length ?? 0}`,
              state.errorHistory?.length ? `Errors: ${state.errorHistory.length}` : null,
            ]
              .filter(Boolean)
              .join("\n"),
        });
      } catch (error) {
        handleError(error, { operation: "mcp-server-status", additionalInfo: { name } });
      }
    });

  // --- mcp tools <name> ---
  mcpCmd
    .command("tools <name>")
    .description("List tools available on an MCP server")
    .action(async (name: string) => {
      try {
        const adapter = new McpAdapter();
        const tools = await adapter.listTools(name);

        router.render(tools, {
          type: "list",
          entity: "mcp-tool",
          format: () =>
            tools.length === 0
              ? "No tools available"
              : tools
                  .map(
                    (t) =>
                      `- ${t.name}: ${t.description || "(no description)"}` +
                      (t.inputSchema ? ` [params: ${JSON.stringify(Object.keys((t.inputSchema as any)["properties"] || {}))}]` : ""),
                  )
                  .join("\n"),
          metadata: { total: tools.length },
        });
      } catch (error) {
        handleError(error, { operation: "list-mcp-tools", additionalInfo: { name } });
      }
    });

  // --- mcp refresh <name> ---
  mcpCmd
    .command("refresh <name>")
    .description("Refresh metadata cache for an MCP server")
    .action(async (name: string) => {
      try {
        const adapter = new McpAdapter();
        await adapter.refreshMetadata(name);
        output.success(`Refreshed metadata for MCP server "${name}"`);
      } catch (error) {
        handleError(error, { operation: "refresh-mcp-metadata", additionalInfo: { name } });
      }
    });

  return mcpCmd;
}
