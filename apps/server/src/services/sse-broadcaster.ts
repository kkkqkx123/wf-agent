/**
 * SSE Broadcaster
 *
 * Implements IEventBroadcaster to receive events from EventManager
 * and distribute them to connected SSE clients. Provides a unified
 * transport-layer abstraction alongside WSManager.
 */

import { type Response } from "express";
import { type ExecutionEvent, type IEventBroadcaster, EventManager } from "./event-manager.js";
import { getOutput } from "../utils/output.js";

interface SSEClient {
  id: string;
  res: Response;
  /** null = subscribe to all executions */
  subscribedExecutionId: string | null;
  connectedAt: Date;
}

export interface SSEBroadcasterOptions {
  /** Max concurrent SSE connections. Default: 100. */
  maxConnections: number;
}

const DEFAULT_OPTIONS: SSEBroadcasterOptions = {
  maxConnections: 100,
};

export class SSEBroadcaster implements IEventBroadcaster {
  readonly name = "sse-broadcaster";
  private clients: Map<string, SSEClient> = new Map();
  private logger = getOutput();
  private eventManager: EventManager;
  private options: SSEBroadcasterOptions;

  constructor(eventManager: EventManager, options?: Partial<SSEBroadcasterOptions>) {
    this.eventManager = eventManager;
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Add a connected SSE client.
   * Returns the assigned client id on success, or null if the connection limit is reached.
   * Replays buffered events for the subscribed execution.
   */
  addClient(res: Response, executionId: string | null): string | null {
    if (this.clients.size >= this.options.maxConnections) {
      this.logger.errorLog(
        `SSE connection rejected: max connections (${this.options.maxConnections}) reached`
      );
      return null;
    }

    const clientId = `sse_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const client: SSEClient = {
      id: clientId,
      res,
      subscribedExecutionId: executionId,
      connectedAt: new Date(),
    };
    this.clients.set(clientId, client);

    // Replay buffered events for this client
    this.eventManager.replayEvents(executionId, (event: ExecutionEvent) => {
      this.sendEvent(client, event);
    });

    this.logger.debugLog(`SSE client connected: ${clientId} (execution: ${executionId || "all"})`);
    return clientId;
  }

  /**
   * Remove a disconnected SSE client.
   */
  removeClient(clientId: string): void {
    this.clients.delete(clientId);
    this.logger.debugLog(`SSE client disconnected: ${clientId}`);
  }

  /**
   * IEventBroadcaster.onEvent — called by EventManager for every emitted event.
   * Forwards the event to all matching SSE clients.
   */
  onEvent(event: ExecutionEvent): void {
    for (const [_clientId, client] of this.clients) {
      if (client.subscribedExecutionId === null || client.subscribedExecutionId === event.executionId) {
        this.sendEvent(client, event);
      }
    }
  }

  /**
   * Send an event to a single SSE client.
   */
  private sendEvent(client: SSEClient, event: ExecutionEvent): void {
    try {
      client.res.write(`event: ${event.type}\n`);
      if (client.subscribedExecutionId === null) {
        // Wildcard: include executionId in data
        client.res.write(`data: ${JSON.stringify({ executionId: event.executionId, ...event.data })}\n\n`);
      } else {
        // Specific execution: omit executionId
        client.res.write(`data: ${JSON.stringify(event.data)}\n\n`);
      }
    } catch (error) {
      this.logger.errorLog(`Failed to send event to SSE client ${client.id}: ${error}`);
      this.clients.delete(client.id);
    }
  }

  /**
   * Get connected client count
   */
  getClientCount(): number {
    return this.clients.size;
  }

  /**
   * Cleanup all SSE connections
   */
  cleanup(): void {
    for (const [_clientId, client] of this.clients) {
      try {
        client.res.end();
      } catch {
        // ignore close errors
      }
    }
    this.clients.clear();
    this.logger.debugLog("SSE broadcaster cleaned up");
  }
}