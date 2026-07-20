/**
 * SSE (Server-Sent Events) Routes
 *
 * Provides real-time event streaming over SSE for Web clients.
 * Clients connect to /api/v1/sse/stream and receive execution events
 * as Server-Sent Events.
 *
 * Uses SSEBroadcaster (IEventBroadcaster) instead of directly subscribing
 * to EventManager, maintaining a consistent transport-layer abstraction.
 */

import { Router, type Request, type Response } from "express";
import { SSEBroadcaster } from "../services/sse-broadcaster.js";

export function createSSERoutes(broadcaster: SSEBroadcaster): Router {
  const router = Router();

  /**
   * SSE event stream endpoint
   * GET /api/v1/sse/stream?executionId=xxx
   *
   * Opens an SSE connection. If executionId is provided, only receives
   * events for that execution. Otherwise receives all events.
   */
  router.get("/", (req: Request, res: Response) => {
    const executionId = req.query["executionId"] as string | undefined;

    // Register client with SSEBroadcaster (checks connection limit)
    const clientId = broadcaster.addClient(res, executionId || null);

    // If connection limit reached, reject with a non-SSE error response
    if (clientId === null) {
      res.writeHead(503, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ error: "Too many SSE connections" }));
      return;
    }

    // Set SSE headers
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
      "X-Accel-Buffering": "no",
    });

    // Send initial connection event
    const connectedPayload = { type: "connected", data: { executionId } };
    res.write(`data: ${JSON.stringify(connectedPayload)}\n\n`);

    // Send keepalive every 30s
    const keepaliveInterval = setInterval(() => {
      res.write(`:keepalive\n\n`);
    }, 30000);

    // Cleanup on client disconnect
    req.on("close", () => {
      broadcaster.removeClient(clientId);
      clearInterval(keepaliveInterval);
    });
  });

  return router;
}