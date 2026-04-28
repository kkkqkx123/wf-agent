/**
 * Streamable HTTP Transport Implementation
 * Handles MCP communication over HTTP with streaming support
 */

import type {
  IMcpTransport,
  TransportEventHandlers,
  StreamableHttpTransportConfig,
} from "./types.js";
import { HttpClient, streamSSE } from "@wf-agent/common-utils";

/**
 * Streamable HTTP Transport
 * Communicates with MCP server via HTTP with streaming responses
 */
export class StreamableHttpTransport implements IMcpTransport {
  readonly type = "streamable-http" as const;

  private handlers: TransportEventHandlers = {};
  private _isConnected = false;
  private config: StreamableHttpTransportConfig;
  private httpClient: HttpClient;
  private abortController: AbortController | null = null;

  constructor(config: StreamableHttpTransportConfig) {
    this.config = config;
    this.httpClient = new HttpClient({
      baseURL: config.url,
      defaultHeaders: config.headers,
    });
  }

  get isConnected(): boolean {
    return this._isConnected;
  }

  /**
   * Start the transport
   * For HTTP, this mainly validates the connection
   */
  async start(): Promise<void> {
    if (this._isConnected) {
      return;
    }

    try {
      // Validate the endpoint is reachable using HEAD request
      await this.httpClient.get("", { method: "HEAD" });
      this._isConnected = true;
    } catch (error) {
      // Even if HEAD fails, we might still be able to communicate
      // So we mark as connected and let actual requests fail if needed
      this._isConnected = true;
    }
  }

  /**
   * Close the transport
   */
  async close(): Promise<void> {
    // Cancel any pending requests
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    this._isConnected = false;
    this.handlers.onClose?.();
  }

  /**
   * Send a message to the MCP server
   * Returns the response data
   */
  async send(message: unknown): Promise<void> {
    if (!this._isConnected) {
      throw new Error("Transport not connected");
    }

    // Create abort controller for this request
    this.abortController = new AbortController();

    try {
      const response = await this.httpClient.post("", message, {
        headers: {
          Accept: "application/json, text/event-stream",
        },
        signal: this.abortController.signal,
      });

      // Handle streaming response
      const contentType = response.headers?.["content-type"] || "";

      if (contentType.includes("text/event-stream") && response.data instanceof ReadableStream) {
        await this.handleStreamingResponse(response.data);
      } else {
        this.handlers.onData?.(response.data);
      }
    } catch (error) {
      // Check if error is due to abort
      if ((error as Error).name === "AbortError") {
        return; // Request was cancelled, not an error
      }
      this.handlers.onError?.(error as Error);
      throw error;
    } finally {
      this.abortController = null;
    }
  }

  /**
   * Handle streaming (SSE) response
   */
  private async handleStreamingResponse(stream: ReadableStream<Uint8Array>): Promise<void> {
    for await (const data of streamSSE(stream)) {
      this.handlers.onData?.(data);
    }
  }

  /**
   * Set event handlers
   */
  setHandlers(handlers: TransportEventHandlers): void {
    this.handlers = handlers;
  }

  /**
   * Send a message and wait for response
   * This is a convenience method for request-response pattern
   */
  async sendAndWait<T = unknown>(message: unknown): Promise<T> {
    return new Promise((resolve, reject) => {
      const originalHandler = this.handlers.onData;

      this.handlers.onData = data => {
        this.handlers.onData = originalHandler;
        resolve(data as T);
      };

      this.handlers.onError = error => {
        this.handlers.onData = originalHandler;
        reject(error);
      };

      this.send(message).catch(reject);
    });
  }
}
