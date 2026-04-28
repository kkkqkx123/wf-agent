/**
 * MCP Client Wrapper
 * Provides a simplified interface for MCP server communication
 */

import type {
  McpTool,
  McpResource,
  McpResourceTemplate,
  McpToolCallResult,
  McpResourceReadResult,
} from "./types.js";
import type { IMcpTransport } from "./transport/index.js";

/**
 * MCP Client
 * Wraps transport layer and provides MCP protocol methods
 */
export class McpClient {
  private transport: IMcpTransport;
  private requestId = 0;
  private pendingRequests = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >();
  private instructions: string | null = null;

  constructor(transport: IMcpTransport) {
    this.transport = transport;
    this.setupTransportHandlers();
  }

  /**
   * Setup transport event handlers
   */
  private setupTransportHandlers(): void {
    this.transport.setHandlers({
      onData: (data) => this.handleResponse(data),
      onError: (error) => this.handleError(error),
      onClose: () => this.handleClose(),
    });
  }

  /**
   * Handle incoming response
   */
  private handleResponse(data: unknown): void {
    const response = data as { id?: number; result?: unknown; error?: unknown };

    if (response.id !== undefined) {
      const pending = this.pendingRequests.get(response.id);
      if (pending) {
        this.pendingRequests.delete(response.id);

        if (response.error) {
          pending.reject(new Error(String(response.error)));
        } else {
          pending.resolve(response.result);
        }
      }
    }
  }

  /**
   * Handle transport error
   */
  private handleError(error: Error): void {
    // Reject all pending requests
    for (const [id, pending] of this.pendingRequests) {
      pending.reject(error);
      this.pendingRequests.delete(id);
    }
  }

  /**
   * Handle transport close
   */
  private handleClose(): void {
    this.handleError(new Error("Transport closed"));
  }

  /**
   * Send a request and wait for response
   */
  private async request<T>(method: string, params?: unknown): Promise<T> {
    const id = this.requestId++;
    const message = {
      jsonrpc: "2.0",
      id,
      method,
      params,
    };

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.transport.send(message).catch(reject);
    });
  }

  /**
   * Connect to the MCP server
   */
  async connect(clientInfo: { name: string; version: string }): Promise<void> {
    await this.transport.start();

    // Initialize connection
    const result = await this.request<{
      instructions?: string;
      capabilities?: unknown;
    }>("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo,
    });

    this.instructions = result.instructions || null;

    // Send initialized notification
    await this.transport.send({
      jsonrpc: "2.0",
      method: "notifications/initialized",
    });
  }

  /**
   * Close the connection
   */
  async close(): Promise<void> {
    await this.transport.close();
    this.pendingRequests.clear();
  }

  /**
   * Get server instructions
   */
  getInstructions(): string | null {
    return this.instructions;
  }

  /**
   * List available tools
   */
  async listTools(): Promise<McpTool[]> {
    const result = await this.request<{ tools: McpTool[] }>("tools/list");
    return result.tools || [];
  }

  /**
   * Call a tool
   */
  async callTool(
    toolName: string,
    args?: Record<string, unknown>,
    timeout?: number
  ): Promise<McpToolCallResult> {
    // Note: timeout handling would need to be implemented at transport level
    return this.request<McpToolCallResult>("tools/call", {
      name: toolName,
      arguments: args,
    });
  }

  /**
   * List available resources
   */
  async listResources(): Promise<McpResource[]> {
    const result = await this.request<{ resources: McpResource[] }>("resources/list");
    return result.resources || [];
  }

  /**
   * List resource templates
   */
  async listResourceTemplates(): Promise<McpResourceTemplate[]> {
    const result = await this.request<{ resourceTemplates: McpResourceTemplate[] }>(
      "resources/templates/list"
    );
    return result.resourceTemplates || [];
  }

  /**
   * Read a resource
   */
  async readResource(uri: string): Promise<McpResourceReadResult> {
    return this.request<McpResourceReadResult>("resources/read", { uri });
  }
}
