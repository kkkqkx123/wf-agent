/**
 * WebSocket Manager
 *
 * Manages WebSocket connections for real-time event streaming.
 * Provides connection management, subscription-based broadcasting,
 * and integration with the EventManager for execution events.
 * Implements IEventBroadcaster to receive events from EventManager
 * without monkey-patching.
 */

import { WebSocketServer, WebSocket } from "ws";
import http from "http";
import { type ExecutionEvent, type IEventBroadcaster } from "./event-manager.js";
import { getOutput } from "../utils/output.js";
import { getAuthConfig } from "../middleware/auth.js";

interface WSClient {
  ws: WebSocket;
  id: string;
  subscribedExecutions: Set<string>;
  connectedAt: Date;
}

export class WSManager implements IEventBroadcaster {
  readonly name = "ws-manager";
  private wss: WebSocketServer | null = null;
  private clients: Map<string, WSClient> = new Map();
  private logger = getOutput();
  /** WebSocket server path */
  private wsPath = "/ws";

  constructor() {
  }

  /**
   * Set the WebSocket server path (call before setup)
   */
  setPath(path: string): void {
    this.wsPath = path;
  }

  /**
   * Get the configured WebSocket server path
   */
  getPath(): string {
    return this.wsPath;
  }

  /**
   * Setup WebSocket server on top of the HTTP server
   */
  setup(server: http.Server): void {
    if (this.wss) {
      this.logger.warn("WebSocket server already initialized");
      return;
    }

    this.wss = new WebSocketServer({ server, path: this.wsPath });

    this.wss.on("connection", (ws: WebSocket, req) => {
      // Authenticate the WebSocket connection
      const authResult = this.authenticate(req);
      if (!authResult.authenticated) {
        ws.close(4001, authResult.error);
        this.logger.warn(`WebSocket connection rejected: ${authResult.error}`);
        return;
      }

      const clientId = `ws_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      const client: WSClient = {
        ws,
        id: clientId,
        subscribedExecutions: new Set(),
        connectedAt: new Date(),
      };
      this.clients.set(clientId, client);

      this.logger.debugLog(`WebSocket client connected: ${clientId} from ${req.socket.remoteAddress}`);

      // Send welcome message
      this.send(clientId, {
        type: "connection",
        data: { clientId, message: "Connected to WF Agent Server" },
        timestamp: new Date().toISOString(),
      });

      // Handle incoming messages (subscription management)
      ws.on("message", (raw) => {
        try {
          const message = JSON.parse(raw.toString()) as Record<string, any>;
          this.handleMessage(clientId, message);
        } catch {
          this.send(clientId, {
            type: "error",
            data: { message: "Invalid message format" },
            timestamp: new Date().toISOString(),
          });
        }
      });

      // Handle disconnection
      ws.on("close", () => {
        this.clients.delete(clientId);
        this.logger.debugLog(`WebSocket client disconnected: ${clientId}`);
      });

      // Handle errors
      ws.on("error", (error) => {
        this.logger.errorLog(`WebSocket error for client ${clientId}: ${error.message}`);
        this.clients.delete(clientId);
      });
    });

    this.logger.infoLog(`WebSocket server initialized on ${this.wsPath}`);
  }

  /**
   * IEventBroadcaster.onEvent — called by EventManager for every emitted event.
   * Forwards the event to all clients subscribed to that execution.
   */
  onEvent(event: ExecutionEvent): void {
    this.broadcast(event.executionId, event);
  }

  /**
   * Authenticate a WebSocket connection request.
   * Checks API key from query parameter (api_key) or from the auth config.
   */
  private authenticate(req: http.IncomingMessage): { authenticated: true } | { authenticated: false; error: string } {
    const authConfig = getAuthConfig();

    // Skip auth if globally disabled
    if (!authConfig.enabled) {
      return { authenticated: true };
    }

    // Extract API key from URL query parameter
    const url = new URL(req.url || "", `http://${req.headers.host || "localhost"}`);
    let apiKey: string | undefined;

    if (authConfig.allowQueryParam) {
      apiKey = url.searchParams.get(authConfig.queryParamName) || undefined;
    }

    if (!apiKey) {
      return {
        authenticated: false,
        error: `Authentication required. Provide API key via ?${authConfig.queryParamName}=<key> query parameter.`,
      };
    }

    if (!authConfig.apiKeys.includes(apiKey)) {
      return { authenticated: false, error: "Invalid API key." };
    }

    return { authenticated: true };
  }

  /**
   * Handle incoming WebSocket messages
   */
  private handleMessage(clientId: string, message: Record<string, any>): void {
    const client = this.clients.get(clientId);
    if (!client) return;

    const msgType = message["type"] as string;
    const execId = message["executionId"] as string | undefined;

    switch (msgType) {
      case "subscribe":
        if (execId) {
          client.subscribedExecutions.add(execId);
          this.send(clientId, {
            type: "subscribed",
            data: { executionId: execId },
            timestamp: new Date().toISOString(),
          });
        }
        break;

      case "unsubscribe":
        if (execId) {
          client.subscribedExecutions.delete(execId);
          this.send(clientId, {
            type: "unsubscribed",
            data: { executionId: execId },
            timestamp: new Date().toISOString(),
          });
        }
        break;

      case "ping":
        this.send(clientId, {
          type: "pong",
          data: { timestamp: Date.now() },
          timestamp: new Date().toISOString(),
        });
        break;

      default:
        this.send(clientId, {
          type: "error",
          data: { message: `Unknown message type: ${msgType}` },
          timestamp: new Date().toISOString(),
        });
    }
  }

  /**
   * Send a message to a specific client
   */
  send(clientId: string, payload: Record<string, any>): boolean {
    const client = this.clients.get(clientId);
    if (!client || client.ws.readyState !== WebSocket.OPEN) {
      return false;
    }
    try {
      client.ws.send(JSON.stringify(payload));
      return true;
    } catch (error) {
      this.logger.errorLog(`Failed to send to client ${clientId}: ${error}`);
      return false;
    }
  }

  /**
   * Broadcast an execution event to all subscribed clients
   */
  broadcast(executionId: string, event: ExecutionEvent): void {
    const payload = {
      type: "execution_event",
      executionId: event.executionId,
      eventType: event.type,
      data: event.data,
      timestamp: event.timestamp,
    };

    for (const [cid, client] of this.clients) {
      if (client.subscribedExecutions.has(executionId) && client.ws.readyState === WebSocket.OPEN) {
        this.send(cid, payload);
      }
    }
  }

  /**
   * Broadcast to ALL connected clients (for global events)
   */
  broadcastAll(payload: Record<string, any>): number {
    let sentCount = 0;
    for (const [cid, _client] of this.clients) {
      if (this.send(cid, payload)) {
        sentCount++;
      }
    }
    return sentCount;
  }

  /**
   * Get connected client count
   */
  getClientCount(): number {
    return this.clients.size;
  }

  /**
   * Get client info
   */
  getClients(): Array<{ id: string; subscribedExecutions: string[]; connectedAt: Date }> {
    return Array.from(this.clients.values()).map((c) => ({
      id: c.id,
      subscribedExecutions: Array.from(c.subscribedExecutions),
      connectedAt: c.connectedAt,
    }));
  }

  /**
   * Cleanup all connections
   */
  cleanup(): void {
    for (const [_clientId, client] of this.clients) {
      try {
        client.ws.close();
      } catch {
        // ignore close errors
      }
    }
    this.clients.clear();

    if (this.wss) {
      this.wss.close();
      this.wss = null;
    }
    this.logger.debugLog("WebSocket server cleaned up");
  }
}