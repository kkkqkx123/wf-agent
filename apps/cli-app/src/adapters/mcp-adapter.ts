/**
 * MCP (Model Context Protocol) Service Adapter
 * Encapsulates MCP server management and diagnostic operations
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  getMcpManager,
  releaseMcpManager,
  type McpServerState,
  type McpTool,
} from "@wf-agent/sdk/services";

/**
 * MCP Adapter
 * Provides CLI-friendly access to MCP server registry and management
 */
export class McpAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get the MCP connection manager instance
   */
  private async getManager() {
    return getMcpManager();
  }

  /**
   * List all registered MCP servers
   */
  async listServers(): Promise<McpServerState[]> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      return manager.getAllServerStates();
    }, "List MCP servers");
  }

  /**
   * Get details of a specific MCP server
   * @param name Server name
   */
  async getServer(name: string): Promise<McpServerState | null> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      const state = manager.getServerState(name);
      if (!state) {
        return null;
      }
      return state;
    }, `Get MCP server "${name}"`);
  }

  /**
   * Connect to an MCP server
   * @param name Server name
   * @param config Server configuration
   */
  async connectServer(name: string, config?: Record<string, unknown>): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();

      let serverConfig: any;
      if (config) {
        serverConfig = config;
      } else {
        throw new Error(
          `No configuration provided for server "${name}". Provide --config or ensure server is defined in MCP settings.`,
        );
      }

      await manager.connectServer(name, serverConfig, "global");
      this.logOperation(`Connected to MCP server "${name}"`);
    }, `Connect to MCP server "${name}"`);
  }

  /**
   * Disconnect from an MCP server
   * @param name Server name
   */
  async disconnectServer(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      await manager.disconnectServer(name);
      this.logOperation(`Disconnected from MCP server "${name}"`);
    }, `Disconnect MCP server "${name}"`);
  }

  /**
   * Get connection status for a server
   * @param name Server name
   */
  async getServerStatus(name: string): Promise<McpServerState | null> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      const state = manager.getServerState(name);
      if (!state) {
        return null;
      }
      return state;
    }, `Get MCP server status "${name}"`);
  }

  /**
   * List tools available on a connected MCP server
   * @param name Server name
   */
  async listTools(name: string): Promise<McpTool[]> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      const state = manager.getServerState(name);
      if (!state) {
        throw new Error(`MCP server "${name}" not found`);
      }
      return state.tools || [];
    }, `List MCP tools for "${name}"`);
  }

  /**
   * Refresh server metadata cache
   * @param name Server name
   */
  async refreshMetadata(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      await manager.refreshServerMetadata(name);
      this.logOperation(`Refreshed metadata for MCP server "${name}"`);
    }, `Refresh MCP server metadata for "${name}"`);
  }

  /**
   * Get connected servers
   */
  async getConnectedServers(): Promise<McpServerState[]> {
    return this.executeWithErrorHandling(async () => {
      const manager = await this.getManager();
      return manager.getConnectedServers();
    }, "List connected MCP servers");
  }

  /**
   * Release the MCP manager reference
   */
  async release(): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await releaseMcpManager();
    }, "Release MCP manager");
  }
}
