/**
 * Message Routes
 * REST API endpoints for message management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { MessageAdapter } from "../adapters/message-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";

export function createMessageRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MessageAdapter>("message");
      const filter: Record<string, any> = {};
      const executionId = getSafeParam(req.query["executionId"] as string);
      const role = getSafeParam(req.query["role"] as string);
      if (executionId) filter["executionId"] = executionId;
      if (role) filter["role"] = role;
      const messages = await adapter.listMessages(filter);
      res.json(successResponse(messages, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/by-execution/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MessageAdapter>("message");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const messages = await adapter.listMessagesByExecution(executionId);
      res.json(successResponse(messages, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/stats/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MessageAdapter>("message");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const stats = await adapter.getMessageStats(executionId);
      res.json(successResponse(stats, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/global-stats", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MessageAdapter>("message");
      const stats = await adapter.getGlobalMessageStats();
      res.json(successResponse(stats, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MessageAdapter>("message");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Message ID is required")); return; }
      const message = await adapter.getMessage(id);
      res.json(successResponse(message, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}