/**
 * SSE Transport Implementation
 * Handles MCP communication over Server-Sent Events
 */

import type { IMcpTransport, TransportEventHandlers, SseTransportConfig } from "./types.js";
import { readSSEStream } from "@wf-agent/common-utils";

/**
 * SSE Transport
 * Communicates with MCP server via Server-Sent Events
 */
export class SseTransport implements IMcpTransport {
  readonly type = "sse" as const;

  private eventSource: EventSource | null = null;
  private handlers: TransportEventHandlers = {};
  private _isConnected = false;
  private config: SseTransportConfig;
  private messageQueue: unknown[] = [];
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private abortController: AbortController | null = null;

  constructor(config: SseTransportConfig) {
    this.config = config;
  }

  get isConnected(): boolean {
    return this._isConnected && this.eventSource !== null;
  }

  /**
   * Start the transport by connecting to SSE endpoint
   */
  async start(): Promise<void> {
    if (this.eventSource) {
      return; // Already connected
    }

    return new Promise((resolve, reject) => {
      try {
        const url = new URL(this.config.url);

        // Create EventSource with custom headers if provided
        // Note: EventSource doesn't support custom headers natively
        // For custom headers, we need to use fetch-based approach
        if (this.config.headers && Object.keys(this.config.headers).length > 0) {
          this.connectWithFetch(url, resolve, reject);
        } else {
          this.connectWithEventSource(url, resolve, reject);
        }
      } catch (error) {
        reject(error);
      }
    });
  }

  /**
   * Connect using native EventSource
   */
  private connectWithEventSource(
    url: URL,
    resolve: () => void,
    reject: (error: Error) => void,
  ): void {
    this.eventSource = new EventSource(url.toString());

    this.eventSource.onopen = () => {
      this._isConnected = true;
      this.reconnectAttempts = 0;
      resolve();
    };

    this.eventSource.onerror = event => {
      this._isConnected = false;

      if (this.reconnectAttempts < this.maxReconnectAttempts) {
        this.reconnectAttempts++;
        console.log(
          `SSE connection error, attempting reconnect (${this.reconnectAttempts}/${this.maxReconnectAttempts})`,
        );
        // EventSource will auto-reconnect
      } else {
        this.handlers.onError?.(
          new Error("SSE connection failed after maximum reconnect attempts"),
        );
        reject(new Error("SSE connection failed"));
      }
    };

    this.eventSource.onmessage = event => {
      try {
        const message = JSON.parse(event.data);
        this.handlers.onData?.(message);
      } catch {
        this.handlers.onData?.(event.data);
      }
    };
  }

  /**
   * Connect using fetch API (for custom headers support)
   */
  private async connectWithFetch(
    url: URL,
    resolve: () => void,
    reject: (error: Error) => void,
  ): Promise<void> {
    // Create abort controller for this connection
    this.abortController = new AbortController();

    try {
      const response = await fetch(url.toString(), {
        method: "GET",
        headers: {
          Accept: "text/event-stream",
          ...this.config.headers,
        },
        signal: this.abortController.signal,
      });

      if (!response.ok) {
        throw new Error(`SSE connection failed: ${response.status} ${response.statusText}`);
      }

      if (!response.body) {
        throw new Error("Response body is null");
      }

      this._isConnected = true;
      this.reconnectAttempts = 0;
      resolve();

      // Read the stream using shared utility
      await readSSEStream(response.body, data => {
        if (this._isConnected) {
          this.handlers.onData?.(data);
        }
      });

      this._isConnected = false;
      this.handlers.onClose?.();
    } catch (error) {
      // Check if error is due to abort
      if ((error as Error).name === "AbortError") {
        this._isConnected = false;
        this.handlers.onClose?.();
        return;
      }
      this._isConnected = false;
      this.handlers.onError?.(error as Error);
      reject(error as Error);
    } finally {
      this.abortController = null;
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
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this._isConnected = false;
    this.handlers.onClose?.();
  }

  /**
   * Send a message to the MCP server
   * Note: SSE is typically one-way (server to client)
   * For two-way communication, we need to use POST requests
   */
  async send(message: unknown): Promise<void> {
    // SSE doesn't support sending messages directly
    // We need to use a separate HTTP POST endpoint
    const url = new URL(this.config.url);

    const response = await fetch(url.toString(), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...this.config.headers,
      },
      body: JSON.stringify(message),
    });

    if (!response.ok) {
      throw new Error(`Failed to send message: ${response.status} ${response.statusText}`);
    }
  }

  /**
   * Set event handlers
   */
  setHandlers(handlers: TransportEventHandlers): void {
    this.handlers = handlers;
  }
}
