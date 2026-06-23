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
  McpServerLifecycle,
} from "../types.js";
import { createTransport } from "../transport/index.js";
import { McpClient } from "./mcp-client.js";
import { McpToolMetadataCache } from "../features/metadata/metadata-cache.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import {
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  updateLastActivity,
  isIdleBeyond,
} from "./connection-state.js";

const logger = createContextualLogger({ component: "MCPConnectionManager" });

/**
 * Connection entry (server state + client + lifecycle metadata)
 */
interface ConnectionEntry {
  state: McpServerState;
  client: McpClient | null;
  config: McpServerConfig;
  lifecycle: McpServerLifecycle;
  idleTimeoutMs: number;
  healthCheckIntervalMs: number;
}

/**
 * MCP Connection Manager
 * Handles connection lifecycle for MCP servers
 */
export class McpConnectionManager {
  private connections = new Map<string, ConnectionEntry>();
  private options: {
    mcpEnabled: boolean;
    maxErrorHistory: number;
    connectionTimeout: number;
    configDebounceDelay: number;
    defaultLifecycle: McpServerLifecycle;
    defaultIdleTimeout: number;
    defaultHealthCheckInterval: number;
  };
  private eventHandlers: McpEventHandler[] = [];
  private clientInfo: { name: string; version: string };
  private idleTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private healthCheckIntervals = new Map<string, ReturnType<typeof setInterval>>();
  private shuttingDown = false;
  private metadataCache: McpToolMetadataCache;
  private connectingPromises = new Map<string, Promise<void>>();

  constructor(
    clientInfo: { name: string; version: string },
    options?: McpManagerOptions,
    metadataCache?: McpToolMetadataCache,
  ) {
    this.clientInfo = clientInfo;
    this.metadataCache = metadataCache ?? new McpToolMetadataCache({
      defaultTtl: 5 * 60 * 1000,
      enableAutoCleanup: true,
      cleanupInterval: 60 * 1000,
      // If autoShutdownExpiredServers is enabled, disconnect servers when their metadata expires
      onExpire: (options?.autoShutdownExpiredServers !== false)
        ? (serverName) => {
            this.disconnectServer(serverName).catch(err => {
              logger.debug(`Auto-shutdown error for expired server "${serverName}":`, { err });
            });
          }
        : undefined,
    });
    this.options = {
      mcpEnabled: options?.mcpEnabled ?? true,
      maxErrorHistory: options?.maxErrorHistory ?? 100,
      connectionTimeout: options?.connectionTimeout ?? 60000,
      configDebounceDelay: options?.configDebounceDelay ?? 500,
      defaultLifecycle: options?.defaultLifecycle ?? "lazy",
      defaultIdleTimeout: options?.defaultIdleTimeout ?? 0,
      defaultHealthCheckInterval: options?.defaultHealthCheckInterval ?? 30,
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
        logger.error("Error in event handler", { error });
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
   * Resolve lifecycle config from server config + manager defaults
   */
  private resolveLifecycle(config: McpServerConfig): {
    lifecycle: McpServerLifecycle;
    idleTimeoutMs: number;
    healthCheckIntervalMs: number;
  } {
    return {
      lifecycle: config.lifecycle ?? this.options.defaultLifecycle,
      idleTimeoutMs: (config.idleTimeout ?? this.options.defaultIdleTimeout) * 1000,
      healthCheckIntervalMs:
        (config.healthCheckInterval ?? this.options.defaultHealthCheckInterval) * 1000,
    };
  }

  /**
   * Connect to a server
   *
   * Behavior depends on lifecycle:
   * - lazy: register metadata only, don't connect (connect on first use)
   * - eager: connect immediately
   * - keep-alive: connect immediately + start health check loop
   */
  async connectServer(
    name: string,
    config: McpServerConfig,
    source: McpServerSource = "global",
    projectPath?: string,
  ): Promise<void> {
    if (this.shuttingDown) {
      logger.warn(`Skipping connectServer("${name}") — manager is shutting down`);
      return;
    }

    const { lifecycle, idleTimeoutMs, healthCheckIntervalMs } = this.resolveLifecycle(config);

    if (!this.options.mcpEnabled) {
      const state = createInitialServerState(name, JSON.stringify(config), source, projectPath);
      state.disabled = true;
      this.connections.set(name, {
        state,
        client: null,
        config,
        lifecycle,
        idleTimeoutMs,
        healthCheckIntervalMs,
      });
      return;
    }

    if (config.disabled) {
      const state = createInitialServerState(name, JSON.stringify(config), source, projectPath);
      state.disabled = true;
      this.connections.set(name, {
        state,
        client: null,
        config,
        lifecycle,
        idleTimeoutMs,
        healthCheckIntervalMs,
      });
      return;
    }

    await this.cleanupServerTimers(name);

    const entry: ConnectionEntry = {
      state: createInitialServerState(name, JSON.stringify(config), source, projectPath),
      client: null,
      config,
      lifecycle,
      idleTimeoutMs,
      healthCheckIntervalMs,
    };
    this.connections.set(name, entry);

    if (lifecycle === "lazy") {
      logger.debug(`Server "${name}" registered in lazy mode — will connect on first use`);
      return;
    }

    await this.doConnect(name, config);

    if (lifecycle === "keep-alive") {
      this.startHealthCheck(name);
    }

    if (lifecycle === "eager" && idleTimeoutMs > 0) {
      this.startIdleTimer(name);
    }
  }

  /**
   * Perform actual transport connection, fetch capabilities, and update state.
   */
  private async doConnect(
    name: string,
    config: McpServerConfig,
  ): Promise<void> {
    const entry = this.connections.get(name);
    if (!entry) return;

    entry.state.status = "connecting";
    this.emitEvent({ type: "server:connecting", serverName: name });

    try {
      const transport = createTransport(config);
      const client = new McpClient(transport);
      entry.client = client;

      await this.withTimeout(
        client.connect(this.clientInfo),
        this.options.connectionTimeout,
        `Connection timeout for server "${name}"`,
      );

      entry.state = updateServerStatus(entry.state, "connected");
      entry.state = clearErrorState(entry.state);
      entry.state.instructions = client.getInstructions() || undefined;
      entry.state = updateLastActivity(entry.state);

      entry.state.tools = await this.fetchTools(client, name, config);
      entry.state.resources = await this.fetchResources(client);
      entry.state.resourceTemplates = await this.fetchResourceTemplates(client);

      // Cache metadata for dynamic context injection
      this.metadataCache.set(name, entry.state);

      this.emitEvent({ type: "server:connected", serverName: name });
      logger.info(`Server "${name}" connected (lifecycle: ${entry.lifecycle})`);
    } catch (error) {
      entry.state = updateServerStatus(entry.state, "disconnected");
      entry.state = addErrorToHistory(
        entry.state,
        error instanceof Error ? error.message : String(error),
      );
      entry.client = null;

      this.emitEvent({
        type: "server:error",
        serverName: name,
        data: error,
      });

      logger.error(`Failed to connect server "${name}":`, { error });
      throw error;
    }
  }

  /**
   * Auto-connect a lazy server if it's not connected, then execute a callback.
   * Returns the result of the callback.
   *
   * Thread-safe: Uses promise-based lock to prevent concurrent connections to lazy servers.
   */
  private async withServer<T>(name: string, fn: (client: McpClient) => Promise<T>): Promise<T> {
    const entry = this.connections.get(name);
    if (!entry) {
      throw new Error(`No server registered: ${name}`);
    }

    if (entry.state.disabled) {
      throw new Error(`Server "${name}" is disabled`);
    }

    const needsConnect = entry.lifecycle === "lazy" && entry.state.status !== "connected";

    if (needsConnect) {
      // Use promise-based lock to prevent concurrent connection attempts
      let connectPromise = this.connectingPromises.get(name);
      if (!connectPromise) {
        logger.debug(`Auto-connecting lazy server "${name}"`);
        connectPromise = this.doConnect(name, entry.config)
          .finally(() => this.connectingPromises.delete(name));
        this.connectingPromises.set(name, connectPromise);
      }
      // Wait for connection to complete (new or existing)
      await connectPromise;
    }

    const reloaded = this.connections.get(name);
    if (!reloaded?.client) {
      throw new Error(`No connection found for server: ${name}`);
    }

    try {
      return await fn(reloaded.client);
    } finally {
      if (reloaded.lifecycle !== "keep-alive") {
        this.resetIdleTimer(name, reloaded);
      }
    }
  }

  /**
   * Start idle timer for a server.
   * Disconnects when idle timeout is reached.
   */
  private startIdleTimer(name: string): void {
    this.cancelIdleTimer(name);
    const entry = this.connections.get(name);
    if (!entry || entry.idleTimeoutMs <= 0) return;

    const timer = setTimeout(() => {
      if (this.shuttingDown) return;
      const current = this.connections.get(name);
      if (
        current?.state.status === "connected" &&
        current.lifecycle !== "keep-alive" &&
        isIdleBeyond(current.state, current.idleTimeoutMs)
      ) {
        logger.debug(`Idle timeout reached for "${name}", disconnecting`);
        this.disconnectServer(name).catch(err => {
          logger.error(`Error during idle disconnect for "${name}":`, { err });
        });
      }
    }, entry.idleTimeoutMs);

    this.idleTimers.set(name, timer);
  }

  /**
   * Reset and restart idle timer after activity.
   */
  private resetIdleTimer(name: string, entry: ConnectionEntry): void {
    if (
      entry.lifecycle !== "keep-alive" &&
      entry.idleTimeoutMs > 0 &&
      entry.state.status === "connected"
    ) {
      this.startIdleTimer(name);
    }
  }

  /**
   * Cancel idle timer if running.
   */
  private cancelIdleTimer(name: string): void {
    const existing = this.idleTimers.get(name);
    if (existing) {
      clearTimeout(existing);
      this.idleTimers.delete(name);
    }
  }

  /**
   * Start health check loop for keep-alive servers.
   */
  private startHealthCheck(name: string): void {
    this.cancelHealthCheck(name);
    const entry = this.connections.get(name);
    if (!entry || entry.lifecycle !== "keep-alive") return;

    const interval = setInterval(async () => {
      if (this.shuttingDown) return;
      await this.runHealthCheck(name);
    }, entry.healthCheckIntervalMs);

    this.healthCheckIntervals.set(name, interval);
  }

  /**
   * Perform a health check on a keep-alive server.
   * Reconnects if disconnected.
   *
   * Strategy options:
   * - "list-tools": Call listTools() (comprehensive check, higher overhead)
   * - "light": Call listResources() as lightweight check (lower overhead)
   */
  private async runHealthCheck(name: string): Promise<void> {
    const entry = this.connections.get(name);
    if (!entry || entry.lifecycle !== "keep-alive") return;

    if (entry.state.status === "connected") {
      try {
        const strategy = entry.config.healthCheckStrategy ?? "list-tools";

        if (strategy === "list-tools") {
          // Comprehensive check: fetch tool list
          await entry.client!.listTools();
        } else if (strategy === "light") {
          // Lightweight check: list resources instead of tools (lower overhead)
          await entry.client!.listResources();
        }

        entry.state = {
          ...entry.state,
          lastHealthCheck: Date.now(),
        };
      } catch {
        logger.warn(`Health check failed for "${name}", will reconnect`);
        await this.reconnectServer(name);
      }
    } else {
      await this.reconnectServer(name);
    }
  }

  /**
   * Reconnect a server (used by keep-alive health check).
   */
  private async reconnectServer(name: string): Promise<void> {
    const entry = this.connections.get(name);
    if (!entry || this.shuttingDown) return;

    try {
      await this.cleanupServerTimers(name);
      entry.state = updateServerStatus(entry.state, "connecting");
      this.emitEvent({ type: "server:connecting", serverName: name });

      const transport = createTransport(entry.config);
      const client = new McpClient(transport);
      entry.client = client;

      await this.withTimeout(
        client.connect(this.clientInfo),
        this.options.connectionTimeout,
        `Reconnection timeout for server "${name}"`,
      );

      entry.state = updateServerStatus(entry.state, "connected");
      entry.state = clearErrorState(entry.state);
      entry.state.instructions = client.getInstructions() || undefined;

      entry.state.tools = await this.fetchTools(client, name, entry.config);
      entry.state.resources = await this.fetchResources(client);
      entry.state.resourceTemplates = await this.fetchResourceTemplates(client);

      this.emitEvent({ type: "server:connected", serverName: name });
      logger.info(`Server "${name}" reconnected`);

      this.startHealthCheck(name);
    } catch (error) {
      entry.state = updateServerStatus(entry.state, "disconnected");
      entry.state = addErrorToHistory(
        entry.state,
        `Reconnect failed: ${error instanceof Error ? error.message : String(error)}`,
      );
      entry.client = null;

      this.emitEvent({
        type: "server:error",
        serverName: name,
        data: error,
      });
      logger.error(`Failed to reconnect server "${name}":`, { error });
    }
  }

  /**
   * Cancel health check interval if running.
   */
  private cancelHealthCheck(name: string): void {
    const existing = this.healthCheckIntervals.get(name);
    if (existing) {
      clearInterval(existing);
      this.healthCheckIntervals.delete(name);
    }
  }

  /**
   * Clean up all timers for a single server.
   */
  private async cleanupServerTimers(name: string): Promise<void> {
    this.cancelIdleTimer(name);
    this.cancelHealthCheck(name);

    const entry = this.connections.get(name);
    if (entry?.client) {
      try {
        await entry.client.close();
      } catch (error) {
        logger.debug(`Error closing client for "${name}" during cleanup:`, {
          error,
        });
      }
      entry.client = null;
    }
  }

  /**
   * Disconnect from a server
   */
  async disconnectServer(name: string): Promise<void> {
    await this.cleanupServerTimers(name);
    this.connections.delete(name);
    // Invalidate cached metadata when disconnecting
    this.metadataCache.invalidate(name);
    this.emitEvent({ type: "server:disconnected", serverName: name });
  }

  /**
   * Disconnect all servers
   */
  async disconnectAll(): Promise<void> {
    this.shuttingDown = true;
    const names = Array.from(this.connections.keys());
    await Promise.all(
      names.map(name =>
        this.disconnectServer(name).catch(err => {
          logger.error(`Error disconnecting "${name}" during shutdown:`, { err });
        }),
      ),
    );
    // Clear all cache on shutdown
    this.metadataCache.clear();
    // Clear all connecting promises
    this.connectingPromises.clear();
    this.shuttingDown = false;
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
    return Array.from(this.connections.values()).map(entry => entry.state);
  }

  /**
   * Get connected servers
   */
  getConnectedServers(): McpServerState[] {
    return this.getAllServerStates().filter(s => s.status === "connected");
  }

  /**
   * Get metadata cache instance
   * Useful for accessing cached tool/resource metadata for dynamic context injection
   */
  getMetadataCache(): McpToolMetadataCache {
    return this.metadataCache;
  }

  /**
   * Refresh server metadata and update cache
   * Useful for servers with dynamic tool/resource lists
   *
   * @param name - Server name
   * @throws Error if server not registered or not connected
   */
  async refreshServerMetadata(name: string): Promise<void> {
    const entry = this.connections.get(name);
    if (!entry) {
      throw new Error(`No server registered: ${name}`);
    }

    if (entry.state.status !== "connected" || !entry.client) {
      throw new Error(`Server "${name}" is not connected`);
    }

    try {
      logger.debug(`Refreshing metadata for server "${name}"`);

      entry.state.tools = await this.fetchTools(entry.client, name, entry.config);
      entry.state.resources = await this.fetchResources(entry.client);
      entry.state.resourceTemplates = await this.fetchResourceTemplates(entry.client);

      // Immediately update cache
      this.metadataCache.set(name, entry.state);

      this.emitEvent({
        type: "server:metadata-refreshed",
        serverName: name,
      });

      logger.info(`Metadata refreshed for server "${name}"`);
    } catch (error) {
      logger.error(`Failed to refresh metadata for "${name}":`, { error });
      throw error;
    }
  }

  /**
   * Call a tool on a server
   */
  async callTool(
    serverName: string,
    toolName: string,
    args?: Record<string, unknown>,
  ): Promise<McpToolCallResult> {
    const entry = this.connections.get(serverName);
    if (!entry) {
      throw new Error(`No server registered: ${serverName}`);
    }

    if (entry.state.disabled) {
      throw new Error(`Server "${serverName}" is disabled`);
    }

    const timeout = (entry.config.timeout ?? 60) * 1000;

    return this.withServer(serverName, async client => {
      const result = await this.withTimeout(
        client.callTool(toolName, args),
        timeout,
        `Tool call timeout for "${serverName}.${toolName}"`,
      );

      const current = this.connections.get(serverName);
      if (current) {
        current.state = updateLastActivity(current.state);
      }

      return result;
    });
  }

  /**
   * Read a resource from a server
   */
  async readResource(serverName: string, uri: string): Promise<McpResourceReadResult> {
    const entry = this.connections.get(serverName);
    if (!entry) {
      throw new Error(`No server registered: ${serverName}`);
    }

    if (entry.state.disabled) {
      throw new Error(`Server "${serverName}" is disabled`);
    }

    return this.withServer(serverName, async client => {
      const result = await client.readResource(uri);

      const current = this.connections.get(serverName);
      if (current) {
        current.state = updateLastActivity(current.state);
      }

      return result;
    });
  }

  /**
   * Fetch tools list with configuration
   */
  private async fetchTools(
    client: McpClient,
    serverName: string,
    config: McpServerConfig,
  ): Promise<McpTool[]> {
    try {
      const tools = await client.listTools();
      const alwaysAllow = config.alwaysAllow || [];
      const disabledTools = config.disabledTools || [];

      return tools.map(tool => ({
        ...tool,
        alwaysAllow: alwaysAllow.includes(tool.name),
        enabledForPrompt: !disabledTools.includes(tool.name),
      }));
    } catch (error) {
      logger.error(`Failed to fetch tools for ${serverName}`, { error });
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
  private withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return Promise.race([
        promise,
        new Promise<T>((_, reject) => {
          timer = setTimeout(() => reject(new Error(message)), timeoutMs);
        }),
      ]);
    } finally {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
    }
  }
}
