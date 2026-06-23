/**
 * MCP Server Executor
 *
 * Executes MCP server operations and manages connection lifecycle.
 * Extends BaseRemoteExecutor for consistency with other remote executors.
 */

import type { RemoteConnectionConfig, RemoteExecutorStatus } from "../remote/types.js";
import { BaseRemoteExecutor } from "../remote/BaseRemoteExecutor.js";
import { McpConnectionManager } from "./core/index.js";
import type { McpServerConfig } from "./types.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpServerExecutor" });

/**
 * MCP Server Executor
 * Manages connection to a single MCP server
 */
export class McpServerExecutor extends BaseRemoteExecutor {
  private manager: McpConnectionManager | null = null;
  private serverName: string;

  constructor(serverName: string) {
    super();
    this.serverName = serverName;
  }

  /**
   * Connect to MCP server
   */
  async connect(config: RemoteConnectionConfig): Promise<void> {
    try {
      if (!this.manager) {
        this.manager = new McpConnectionManager(
          { name: "wf-agent-sdk", version: "1.0.0" },
          { mcpEnabled: true }
        );
      }

      const mcpConfig = config as unknown as McpServerConfig;

      await this.manager.connectServer(this.serverName, mcpConfig);
      this.connected = true;

      logger.debug("MCP server connected", { serverName: this.serverName });
    } catch (error) {
      this.connected = false;
      logger.error("Failed to connect MCP server", {
        serverName: this.serverName,
        error,
      });
      throw error;
    }
  }

  /**
   * Disconnect from MCP server
   */
  async disconnect(): Promise<void> {
    try {
      if (this.manager) {
        await this.manager.disconnectServer(this.serverName);
      }
      this.connected = false;
      logger.debug("MCP server disconnected", { serverName: this.serverName });
    } catch (error) {
      logger.error("Failed to disconnect MCP server", {
        serverName: this.serverName,
        error,
      });
      throw error;
    }
  }

  /**
   * Call MCP method
   */
  async call<TReq, TResp>(method: string, request: TReq): Promise<TResp> {
    if (!this.manager) {
      throw new Error("Manager not initialized. Call connect() first.");
    }

    if (!this.connected) {
      throw new Error("Not connected to MCP server");
    }

    if (method === "tools/call") {
      const toolCall = request as Record<string, unknown>;
      return this.manager.callTool(
        this.serverName,
        toolCall['name'] as string,
        toolCall['arguments'] as Record<string, unknown> | undefined
      ) as Promise<TResp>;
    }

    if (method === "resources/read") {
      const resourceRead = request as Record<string, unknown>;
      return this.manager.readResource(this.serverName, resourceRead['uri'] as string) as Promise<TResp>;
    }

    throw new Error(`Unsupported method: ${method}`);
  }

  /**
   * Check if connected
   */
  override isConnected(): boolean {
    if (!this.manager || !this.connected) {
      return false;
    }

    const state = this.manager.getServerState(this.serverName);
    return state?.status === "connected";
  }

  /**
   * Get executor status
   */
  override getStatus(): RemoteExecutorStatus {
    if (!this.manager) {
      return "disconnected";
    }

    const state = this.manager.getServerState(this.serverName);
    return (state?.status ?? "disconnected") as RemoteExecutorStatus;
  }

  /**
   * Get the MCP connection manager instance
   * Useful for direct access to advanced features
   */
  getManager(): McpConnectionManager | null {
    return this.manager;
  }

  /**
   * Get server name
   */
  getServerName(): string {
    return this.serverName;
  }
}
