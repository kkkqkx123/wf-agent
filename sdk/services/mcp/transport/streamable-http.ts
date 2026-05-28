/**
 * Streamable HTTP Transport Implementation
 * Handles MCP communication over HTTP with streaming support
 */

import type {
  IMcpTransport,
  TransportEventHandlers,
  StreamableHttpTransportConfig,
} from "./types.js";
import { HttpClient, streamSSE } from "../../http/index.js";
import { executeWithRetry, type RetryConfig } from "../../http/retry-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "StreamableHttpTransport" });

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
  private retryConfig: RetryConfig;

  constructor(config: StreamableHttpTransportConfig) {
    this.config = config;
    this.httpClient = new HttpClient({
      baseURL: config.url,
      defaultHeaders: config.headers,
    });
    // Default retry configuration
    this.retryConfig = {
      maxRetries: 3,
      baseDelay: 1000,
      maxDelay: 10000,
    };
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
      logger.debug("Transport started successfully", { url: this.config.url });
    } catch (error) {
      logger.error("HEAD request failed — server unreachable", {
        url: this.config.url,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
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
      // Use retry handler for automatic retries
      const response = await executeWithRetry(
        async () => {
          return await this.httpClient.post("", message, {
            headers: {
              Accept: "application/json, text/event-stream",
            },
            signal: this.abortController!.signal,
          });
        },
        this.retryConfig,
      );

      // Handle streaming response
      const contentType = response.headers?.["content-type"] || "";

      if (contentType.includes("text/event-stream") && response.data instanceof ReadableStream) {
        await this.handleStreamingResponse(response.data);
      } else {
        this.handlers.onData?.(response.data);
      }

      logger.debug("Message sent successfully", { url: this.config.url });
    } catch (error) {
      // Check if error is due to abort
      if ((error as Error).name === "AbortError") {
        logger.debug("Request aborted", { url: this.config.url });
        return; // Request was cancelled, not an error
      }

      logger.error("Failed to send message after retries", {
        url: this.config.url,
        error: error instanceof Error ? error.message : String(error),
      });

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

}
