/**
 * Progress Routes
 * REST API endpoints for execution progress tracking.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { ProgressTrackingAdapter } from "../adapters/progress-tracking-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";
import type { ID } from "@wf-agent/types";

export function createProgressRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ProgressTrackingAdapter>("progress");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const result = await adapter.getProgress(executionId as ID);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:executionId/status", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ProgressTrackingAdapter>("progress");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const result = await adapter.getExecutionStatus(executionId as ID);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}