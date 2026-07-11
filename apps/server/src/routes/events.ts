/**
 * Events Routes
 *
 * Server-Sent Events (SSE) endpoints for real-time execution updates:
 * - GET /events/executions/:id - Stream execution events
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import { EventManager, type ExecutionEvent } from "../services/event-manager.js";
import { getSafeParam } from "./route-helpers.js";

export function createEventsRoutes(
  _container: ServerDependencyContainer
): Router {
  const router = Router();
  const eventManager = EventManager.getInstance();

  /**
   * Stream execution events
   * GET /events/executions/:id
   *
   * Returns Server-Sent Events stream for real-time updates:
   * - status: Execution state changes (running, paused, cancelled, completed)
   * - log: New log entries
   * - progress: Progress updates (current step, total steps)
   * - error: Execution errors
   * - complete: Execution finished
   *
   * Client usage:
   * ```javascript
   * const eventSource = new EventSource('/api/v1/events/executions/exec_123');
   * eventSource.addEventListener('status', (e) => {
   *   console.log('Status:', JSON.parse(e.data));
   * });
   * eventSource.addEventListener('log', (e) => {
   *   console.log('Log:', JSON.parse(e.data));
   * });
   * eventSource.addEventListener('error', (e) => {
   *   console.error('Connection error');
   * });
   * ```
   */
  router.get("/executions/:id", (req: Request, res: Response) => {
    const id = getSafeParam(req.params["id"]);

    if (!id) {
      res.status(400).json({
        success: false,
        error: {
          code: "VALIDATION_ERROR",
          message: "Execution ID is required",
        },
      });
      return;
    }

    // Set SSE headers
    res.setHeader("Content-Type", "text/event-stream");
    res.setHeader("Cache-Control", "no-cache");
    res.setHeader("Connection", "keep-alive");
    res.setHeader("Access-Control-Allow-Origin", "*");

    // Send initial connection event
    const connectionEvent: ExecutionEvent = {
      type: "status",
      executionId: id,
      timestamp: new Date().toISOString(),
      data: { message: "Connected to event stream" },
    };
    res.write(`data: ${JSON.stringify(connectionEvent)}\n\n`);

    // Subscribe to events
    const unsubscribe = eventManager.subscribe(id, (event: ExecutionEvent) => {
      // Send event as SSE
      try {
        res.write(`data: ${JSON.stringify(event)}\n\n`);
      } catch (error) {
        // Client disconnected, unsubscribe
        unsubscribe();
      }
    });

    // Handle client disconnect
    req.on("close", () => {
      unsubscribe();
      res.end();
    });

    // Keep connection alive with heartbeat
    const heartbeatInterval = setInterval(() => {
      try {
        res.write(":heartbeat\n\n");
      } catch (error) {
        clearInterval(heartbeatInterval);
        unsubscribe();
      }
    }, 30000); // Every 30 seconds

    // Cleanup on response end
    res.on("finish", () => {
      clearInterval(heartbeatInterval);
      unsubscribe();
    });
  });

  return router;
}
