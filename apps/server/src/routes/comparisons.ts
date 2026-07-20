/**
 * Execution Comparison Routes
 * REST API endpoints for comparing workflow executions.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { ExecutionComparisonAdapter } from "../adapters/execution-comparison-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import type { ID } from "@wf-agent/types";

export function createComparisonRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.post("/two", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ExecutionComparisonAdapter>("comparison");
      const { exec1Id, exec2Id } = req.body || {};
      if (!exec1Id || !exec2Id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Both exec1Id and exec2Id are required")); return; }
      const result = await adapter.compareExecutions(exec1Id as ID, exec2Id as ID);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/range", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ExecutionComparisonAdapter>("comparison");
      const { execIds } = req.body || {};
      if (!execIds || !Array.isArray(execIds) || execIds.length < 2) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "At least 2 execution IDs are required"));
        return;
      }
      const result = await adapter.compareRange(execIds as ID[]);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/trend", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ExecutionComparisonAdapter>("comparison");
      const { execIds } = req.body || {};
      if (!execIds || !Array.isArray(execIds) || execIds.length < 2) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "At least 2 execution IDs are required"));
        return;
      }
      const result = await adapter.analyzePerformanceTrend(execIds as ID[]);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}