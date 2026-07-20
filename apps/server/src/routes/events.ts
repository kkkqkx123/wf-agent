/**
 * Event Routes
 * REST API endpoints for event management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { EventAdapter } from "../adapters/event-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createEventRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<EventAdapter>("event");
      const filter: Record<string, any> = {};
      const type = getSafeParam(req.query["type"] as string);
      const executionId = getSafeParam(req.query["executionId"] as string);
      const workflowId = getSafeParam(req.query["workflowId"] as string);
      if (type) filter["type"] = type;
      if (executionId) filter["executionId"] = executionId;
      if (workflowId) filter["workflowId"] = workflowId;
      const limit = getIntParam(req.query["limit"] as string, 50);
      filter["limit"] = limit;
      const events = await adapter.listEvents(filter);
      res.json(successResponse(events, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/stats", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<EventAdapter>("event");
      const filter: Record<string, any> = {};
      const type = getSafeParam(req.query["type"] as string);
      const executionId = getSafeParam(req.query["executionId"] as string);
      const workflowId = getSafeParam(req.query["workflowId"] as string);
      if (type) filter["type"] = type;
      if (executionId) filter["executionId"] = executionId;
      if (workflowId) filter["workflowId"] = workflowId;
      const stats = await adapter.getEventStats(filter);
      res.json(successResponse(stats, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/trim", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<EventAdapter>("event");
      const maxSize = getIntParam(req.body?.maxSize as string, 1000);
      const removed = await adapter.trimEventHistory(maxSize);
      res.json(successResponse({ maxSize, removed }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<EventAdapter>("event");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Event ID is required")); return; }
      const event = await adapter.getEvent(id);
      res.json(successResponse(event, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}