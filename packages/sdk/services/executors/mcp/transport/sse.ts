/**
 * SSE Transport Implementation
 * Handles MCP communication over Server-Sent Events
 */

import type { IMcpTransport, TransportEventHandlers, SseTransportConfig } from "./types.js";
import { readSSEStream } from "../../../transport/http/index.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MCPSSETransport" });

/**
 * Transport state machine
 */
type TransportState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closing"
  | "closed";

/**
 * SSE Transport
 * Communicates with MCP server via Server-Sent Events
 */
export class SseTransport implements IMcpTransport {
  readonly type = "sse" as const;

  private eventSource: EventSource | null = null;
  private handlers: TransportEventHandlers = {};
  private config: SseTransportConfig;
  private abortController: AbortController | null = null;

  // State management
  private _state: TransportState = "disconnected";
  private _isClosing = false;
  private pendingMessages: unknown[] = [];

  // Reconnection configuration
  private reconnectAttempts = 0;
  private maxReconnectAttempts: number;
  private initialDelay: number;
  private maxDelay: number;
  private backoffMultiplier: number;

  constructor(config: SseTransportConfig) {
    this.config = config;

    // Initialize reconnection configuration with defaults
    const reconnection = config.reconnection || {};
    this.maxReconnectAttempts = reconnection.maxAttempts ?? 5;
    this.initialDelay = reconnection.initialDelay ?? 1000;
    this.maxDelay = reconnection.maxDelay ?? 30000;
    this.backoffMultiplier = reconnection.backoffMultiplier ?? 2;
  }

  get isConnected(): boolean {
    return this._state === "connected";
  }

  get state(): TransportState {
    return this._state;
  }

  private setState(newState: TransportState): void {
    const oldState = this._state;
    this._state = newState;
    logger.debug(`Transport state changed: ${oldState} -> ${newState}`);
  }

  /**
   * Start the transport by connecting to SSE endpoint
   */
  async start(): Promise<void> {
    if (this._state === "connected" || this._state === "connecting") {
      return; // Already connected or connecting
    }

    this.setState("connecting");

    return new Promise((resolve, reject) => {
      try {
        const url = new URL(this.config.url);

        // Create EventSource with custom headers if provided
        // Note: EventSource doesn't support custom headers natively
        // For custom headers, we need to use fetch-based approach
        if (this.config.headers && Object.keys(this.config.headers).length > 0) {
          this.connectWithFetch(url, resolve, reject).catch(error => {
            this.setState("disconnected");
            reject(error);
          });
        } else {
          this.connectWithEventSource(url, resolve, reject);
        }
      } catch (error) {
        this.setState("disconnected");
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
      this.setState("connected");
      this.reconnectAttempts = 0;
      resolve();
    };

    this.eventSource.onerror = event => {
      const wasConnected = this._state === "connected";
      this.setState("disconnected");

      if (wasConnected && this.reconnectAttempts < this.maxReconnectAttempts) {
        this.setState("reconnecting");
        this.reconnectAttempts++;
        const delay = Math.min(
          this.initialDelay * Math.pow(this.backoffMultiplier, this.reconnectAttempts - 1),
          this.maxDelay,
        );
        logger.warn(
          `SSE connection error (type: ${event.type}), attempting reconnect (${this.reconnectAttempts}/${this.maxReconnectAttempts}) in ${delay}ms`,
        );
        // EventSource will auto-reconnect, but we track the state
        setTimeout(() => {
          if (this._state === "reconnecting") {
            this.setState("disconnected");
            this.handlers.onError?.(
              new Error("SSE connection failed after maximum reconnect attempts"),
            );
          }
        }, delay);
      } else if (wasConnected) {
        this.handlers.onError?.(
          new Error("SSE connection failed after maximum reconnect attempts"),
        );
        reject(new Error("SSE connection failed"));
      }
    };

    this.eventSource.onmessage = event => {
      try {
        const message = JSON.parse(event.data);
        this.handleIncomingMessage(message);
      } catch {
        this.handleIncomingMessage(event.data);
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
    const attemptConnection = async (): Promise<void> => {
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

        this.setState("connected");
        this.reconnectAttempts = 0;

        // Only resolve on first successful connection
        if (this.reconnectAttempts === 0) {
          resolve();
        }

        // Read the stream using shared utility
        await readSSEStream(response.body, (data: unknown) => {
          if (this._state === "connected") {
            this.handleIncomingMessage(data);
          }
        });

        // Stream ended, attempt reconnect
        this.setState("disconnected");
        await this.attemptReconnect(attemptConnection, reject);
      } catch (error) {
        if ((error as Error).name === "AbortError") {
          this.setState("closed");
          this.handlers.onClose?.();
          return;
        }

        this.setState("disconnected");
        await this.attemptReconnect(attemptConnection, reject);
      } finally {
        // Keep abortController reference until fully closed
        if (this._state !== "closing" && this._state !== "closed") {
          this.abortController = null;
        }
      }
    };

    await attemptConnection();
  }

  /**
   * Attempt to reconnect with exponential backoff
   */
  private async attemptReconnect(
    attemptFn: () => Promise<void>,
    reject: (error: Error) => void,
  ): Promise<void> {
    if (this._isClosing) {
      return;
    }

    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      const delay = Math.min(
        this.initialDelay * Math.pow(this.backoffMultiplier, this.reconnectAttempts - 1),
        this.maxDelay,
      );

      this.setState("reconnecting");
      logger.warn(
        `SSE connection lost, attempting reconnect (${this.reconnectAttempts}/${this.maxReconnectAttempts}) in ${delay}ms`,
      );

      await new Promise(resolve => setTimeout(resolve, delay));

      // Check if closing during wait
      if (this._isClosing) {
        return;
      }

      await attemptFn();
    } else {
      this.handlers.onError?.(new Error("SSE connection failed after maximum reconnect attempts"));
      reject(new Error("SSE connection failed"));
    }
  }

  /**
   * Handle incoming messages with proper state management
   */
  private handleIncomingMessage(data: unknown): void {
    if (this._isClosing) {
      // Buffer messages during closing
      this.pendingMessages.push(data);
      return;
    }

    if (this._state === "connected") {
      this.handlers.onData?.(data);
    }
  }

  /**
   * Close the transport
   */
  async close(): Promise<void> {
    this._isClosing = true;
    this.setState("closing");

    // Cancel any pending requests
    if (this.abortController) {
      this.abortController.abort();
    }
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    // Process any pending messages before closing
    while (this.pendingMessages.length > 0) {
      const msg = this.pendingMessages.shift();
      this.handlers.onData?.(msg);
    }

    this.setState("closed");
    this.handlers.onClose?.();

    // Clear abortController after everything is done
    this.abortController = null;
    this._isClosing = false;
  }

  /**
   * Send a message to the MCP server
   * Note: SSE is one-way (server to client), so we use a separate POST endpoint
   * According to MCP spec, messages should be sent to a dedicated message endpoint
   */
  async send(message: unknown): Promise<void> {
    // For SSE transport, we need to send messages to a separate endpoint
    // Use configured messageEndpoint if provided, otherwise derive from SSE URL
    let messageUrl: string;

    if (this.config.messageEndpoint) {
      messageUrl = this.config.messageEndpoint;
    } else {
      const baseUrl = this.config.url.replace(/\/sse\/?$/, "");
      messageUrl = `${baseUrl}/message`;
    }

    try {
      const response = await fetch(messageUrl, {
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
    } catch (error) {
      logger.error("Failed to send message via SSE transport", {
        error: error instanceof Error ? error.message : String(error),
        messageUrl,
      });
      throw error;
    }
  }

  /**
   * Set event handlers
   */
  setHandlers(handlers: TransportEventHandlers): void {
    this.handlers = handlers;
  }
}
