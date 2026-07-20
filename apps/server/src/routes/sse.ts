/**
 * SSE (Server-Sent Events) Routes
 *
 * Provides real-time event streaming over SSE for Web clients.
 * Clients connect to /api/v1/events/stream and receive execution events
 * as Server-Sent Events.
 */

import { Router, type Request, type Response } from "express";
import { EventManager, type ExecutionEvent } from "../services/event-manager.js";

export function createSSERoutes(): Router {
  const router = Router();

  /**
   * SSE event stream endpoint
   * GET /api/v1/events/stream?executionId=xxx
   *
   * Opens an SSE connection. If executionId is provided, only receives
   * events for that execution. Otherwise receives all events.
   */
  router.get("/", (req: Request, res: Response) => {
    const executionId = req.query["executionId"] as string | undefined;

    // Set SSE headers
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
      "X-Accel-Buffering": "no",
    });

    // Send initial connection event
    res.write(`data: ${JSON.stringify({ type: "connected", data: { executionId } })}\n\n`);

    // Send keepalive every 30s
    const keepaliveInterval = setInterval(() => {
      res.write(`:keepalive\n\n`);
    }, 30000);

    // Subscribe to events
    const eventManager = EventManager.getInstance();

    if (executionId) {
      // Subscribe to specific execution
      const unsubscribe = eventManager.subscribe(executionId, (event: ExecutionEvent) => {
        res.write(`event: ${event.type}\n`);
        res.write(`data: ${JSON.stringify(event.data)}\n\n`);
      });

      // Cleanup on client disconnect
      req.on("close", () => {
        unsubscribe();
        clearInterval(keepaliveInterval);
      });
    } else {
      // Subscribe to all - use a global subscription pattern
      // Forward via EventManager internal emit for all executions
      const unsubscribe = eventManager.subscribe("*", (event: ExecutionEvent) => {
        res.write(`event: ${event.type}\n`);
        res.write(`data: ${JSON.stringify({ executionId: event.executionId, ...event.data })}\n\n`);
      });

      // Cleanup on client disconnect
      req.on("close", () => {
        unsubscribe();
        clearInterval(keepaliveInterval);
      });
    }
  });

  return router;
}