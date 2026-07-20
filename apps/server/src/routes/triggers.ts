/**
 * Trigger Routes
 * REST API endpoints for trigger management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { TriggerAdapter } from "../adapters/trigger-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";

export function createTriggerRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TriggerAdapter>("trigger");
      const triggers = await adapter.listTriggers();
      res.json(successResponse(triggers, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/by-execution/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TriggerAdapter>("trigger");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const triggers = await adapter.listTriggersByWorkflowExecution(executionId);
      res.json(successResponse(triggers, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TriggerAdapter>("trigger");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Trigger ID is required")); return; }
      const trigger = await adapter.getTrigger(id);
      res.json(successResponse(trigger, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/:id/enable", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TriggerAdapter>("trigger");
      const id = getSafeParam(req.params["id"]);
      const executionId = req.body?.executionId as string;
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Trigger ID is required")); return; }
      await adapter.enableTrigger(executionId || "", id);
      res.json(successResponse({ triggerId: id, status: "enabled" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/:id/disable", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TriggerAdapter>("trigger");
      const id = getSafeParam(req.params["id"]);
      const executionId = req.body?.executionId as string;
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Trigger ID is required")); return; }
      await adapter.disableTrigger(executionId || "", id);
      res.json(successResponse({ triggerId: id, status: "disabled" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}