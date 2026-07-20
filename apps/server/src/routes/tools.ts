/**
 * Tool Routes
 * REST API endpoints for tool management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { ToolAdapter } from "../adapters/tool-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createToolRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ToolAdapter>("tool");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.list({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ToolAdapter>("tool");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Tool ID is required")); return; }
      const result = await adapter.get(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/:id/validate", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ToolAdapter>("tool");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Tool ID is required")); return; }
      const result = await adapter.validateTool(id, req.body?.config);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}