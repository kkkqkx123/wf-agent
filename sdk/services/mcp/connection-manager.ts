/**
 * MCP Connection Manager
 * Manages connections to MCP servers
 */

import type {
  McpServerConfig,
  McpServerState,
  McpTool,
  McpResource,
  McpResourceTemplate,
  McpToolCallResult,
  McpResourceReadResult,
  McpManagerOptions,
  McpEventHandler,
  McpServerSource,
} from "./types.js";
import { createTransport } from "./transport/index.js";
import { McpClient } from "./mcp-client.js";
import {
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
} from "./connection-state.js";

/**
 * Connection entry (server state + client)
 */
interface ConnectionEntry {
  state: McpServerState;
  client: McpClient | null;
}

/**
 * MCP Connection Manager
 * Handles connection lifecycle for MCP servers
 */
export class McpConnectionManager {
  private connections = new Map<string, ConnectionEntry>();
  private options: Required<McpManagerOptions>;
  private eventHandlers: McpEventHandler[] = [];
  private clientInfo: { name: string; version: string };

  constructor(
    clientInfo: { name: string; version: string },
    options?: McpManagerOptions
  ) {
    this.clientInfo = clientInfo;
    this.options = {
      mcpEnabled: options?.mcpEnabled ?? true,
      maxErrorHistory: options?.maxErrorHistory ?? 100,
      connectionTimeout: options?.connectionTimeout ?? 60000,
      configDebounceDelay: options?.configDebounceDelay ?? 500,
    };
  }

  /**
   * Add event handler
   */
  addEventHandler(handler: McpEventHandler): void {
    this.eventHandlers.push(handler);
  }

  /**
   * Remove event handler
   */
  removeEventHandler(handler: McpEventHandler): void {
    const index = this.eventHandlers.indexOf(handler);
    if (index >= 0) {
      this.eventHandlers.splice(index, 1);
    }
  }

  /**
   * Emit event to handlers
   */
  private emitEvent(type: McpEventHandler extends (e: infer E) => void ? E : never): void {
    for (const handler of this.eventHandlers) {
      try {
        handler(type);
      } catch (error) {
        console.error("Error in event handler:", error);
      }
    }
  }

  /**
   * Check if MCP is enabled
   */
  isMcpEnabled(): boolean {
    return this.options.mcpEnabled;
  }

  /**
   * Set MCP enabled state
   */
  setMcpEnabled(enabled: boolean): void {
    this.options.mcpEnabled = enabled;
  }

  /**
   * Connect to a server
   */
  async connectServer(
    name: string,
    config: McpServerConfig,
    source: McpServerSource = "global",
    projectPath?: string
  ): Promise<void> {
    // Check if MCP is enabled
    if (!this.options.mcpEnabled) {
      // Create disconnected entry
      const state = createInitialServerState(name, JSON.stringify(config), source, projectPath);
      state.disabled = true;
      this.connections.set(name, { state, client: null });
      return;
    }

    // Check if server is disabled
    if (config.disabled) {
      const state = createInitialServerState(name, JSON.stringify(config), source, projectPath);
      state.disabled = true;
      this.connections.set(name, { state, client: null });
      return;
    }

    // Remove existing connection
    await this.disconnectServer(name);

    // Create initial state
    const state = createInitialServerState(name, JSON.stringify(config), source, projectPath);
    state.status = "connecting";

    this.emitEvent({ type: "server:connecting", serverName: name });

    try {
      // Create transport
      const transport = createTransport(config as any);
      const client = new McpClient(transport);

      // Store connecting state
      this.connections.set(name, { state, client });

      // Connect with timeout
      await this.withTimeout(
        client.connect(this.clientInfo),
        this.options.connectionTimeout,
        `Connection timeout for server "${name}"`
      );

      // Update state to connected
      const entry = this.connections.get(name);
      if (entry) {
        entry.state = updateServerStatus(entry.state, "connected");
        entry.state = clearErrorState(entry.state);
        entry.state.instructions = client.getInstructions() || undefined;

        // Fetch capabilities
        entry.state.tools = await this.fetchTools(client, name, config);
        entry.state.resources = await this.fetchResources(client);
        entry.state.resourceTemplates = await this.fetchResourceTemplates(client);
      }

      this.emitEvent({ type: "server:connected", serverName: name });
    } catch (error) {
      const entry = this.connections.get(name);
      if (entry) {
        entry.state = updateServerStatus(entry.state, "disconnected");
        entry.state = addErrorToHistory(
          entry.state,
          error instanceof Error ? error.message : String(error)
        );
      }

      this.emitEvent({
        type: "server:error",
        serverName: name,
        data: error,
      });

      throw error;
    }
  }

  /**
   * Disconnect from a server
   */
  async disconnectServer(name: string): Promise<void> {
    const entry = this.connections.get(name);
    if (!entry) return;

    if (entry.client) {
      try {
        await entry.client.close();
      } catch (error) {
        console.error(`Error closing connection for "${name}":`, error);
      }
    }

    this.connections.delete(name);
    this.emitEvent({ type: "server:disconnected", serverName: name });
  }

  /**
   * Disconnect all servers
   */
  async disconnectAll(): Promise<void> {
    const names = Array.from(this.connections.keys());
    await Promise.all(names.map((name) => this.disconnectServer(name)));
  }

  /**
   * Get server state
   */
  getServerState(name: string): McpServerState | undefined {
    return this.connections.get(name)?.state;
  }

  /**
   * Get all server states
   */
  getAllServerStates(): McpServerState[] {
    return Array.from(this.connections.values()).map((entry) => entry.state);
  }

  /**
   * Get connected servers
   */
  getConnectedServers(): McpServerState[] {
    return this.getAllServerStates().filter((s) => s.status === "connected");
  }

  /**
   * Call a tool on a server
   */
  async callTool(
    serverName: string,
    toolName: string,
    args?: Record<string, unknown>
  ): Promise<McpToolCallResult> {
    const entry = this.connections.get(serverName);
    if (!entry || !entry.client) {
      throw new Error(`No connection found for server: ${serverName}`);
    }

    if (entry.state.disabled) {
      throw new Error(`Server "${serverName}" is disabled`);
    }

    // Get timeout from config
    const config = JSON.parse(entry.state.config) as McpServerConfig;
    const timeout = (config.timeout ?? 60) * 1000;

    return this.withTimeout(
      entry.client.callTool(toolName, args),
      timeout,
      `Tool call timeout for "${serverName}.${toolName}"`
    );
  }

  /**
   * Read a resource from a server
   */
  async readResource(serverName: string, uri: string): Promise<McpResourceReadResult> {
    const entry = this.connections.get(serverName);
    if (!entry || !entry.client) {
      throw new Error(`No connection found for server: ${serverName}`);
    }

    if (entry.state.disabled) {
      throw new Error(`Server "${serverName}" is disabled`);
    }

    return entry.client.readResource(uri);
  }

  /**
   * Fetch tools list with configuration
   */
  private async fetchTools(
    client: McpClient,
    serverName: string,
    config: McpServerConfig
  ): Promise<McpTool[]> {
    try {
      const tools = await client.listTools();
      const alwaysAllow = config.alwaysAllow || [];
      const disabledTools = config.disabledTools || [];

      return tools.map((tool) => ({
        ...tool,
        alwaysAllow: alwaysAllow.includes(tool.name),
        enabledForPrompt: !disabledTools.includes(tool.name),
      }));
    } catch (error) {
      console.error(`Failed to fetch tools for ${serverName}:`, error);
      return [];
    }
  }

  /**
   * Fetch resources list
   */
  private async fetchResources(client: McpClient): Promise<McpResource[]> {
    try {
      return await client.listResources();
    } catch {
      return [];
    }
  }

  /**
   * Fetch resource templates list
   */
  private async fetchResourceTemplates(client: McpClient): Promise<McpResourceTemplate[]> {
    try {
      return await client.listResourceTemplates();
    } catch {
      return [];
    }
  }

  /**
   * Wrap promise with timeout
   */
  private async withTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string
  ): Promise<T> {
    return Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        setTimeout(() => reject(new Error(message)), timeoutMs);
      }),
    ]);
  }
}
