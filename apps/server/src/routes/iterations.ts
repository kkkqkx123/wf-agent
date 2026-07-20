/**
 * Agent Loop Iteration Routes
 * REST API endpoints for iteration analysis.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { IterationAnalysisAdapter } from "../adapters/iteration-analysis-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";
import type { ID } from "@wf-agent/types";

export function createIterationRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/:agentLoopId/summary", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<IterationAnalysisAdapter>("iteration-analysis");
      const agentLoopId = getSafeParam(req.params["agentLoopId"]);
      if (!agentLoopId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const summary = await adapter.getIterationHistorySummary(agentLoopId as ID);
      res.json(successResponse(summary, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:agentLoopId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<IterationAnalysisAdapter>("iteration-analysis");
      const agentLoopId = getSafeParam(req.params["agentLoopId"]);
      if (!agentLoopId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const limit = getIntParam(req.query["limit"] as string, 20);
      const iterations = await adapter.listIterations(agentLoopId as ID, limit);
      res.json(successResponse(iterations, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:agentLoopId/analyze/decisions", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<IterationAnalysisAdapter>("iteration-analysis");
      const agentLoopId = getSafeParam(req.params["agentLoopId"]);
      if (!agentLoopId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const analysis = await adapter.analyzeDecisions(agentLoopId as ID);
      res.json(successResponse(analysis, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:agentLoopId/analyze/paths", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<IterationAnalysisAdapter>("iteration-analysis");
      const agentLoopId = getSafeParam(req.params["agentLoopId"]);
      if (!agentLoopId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const analysis = await adapter.analyzeExecutionPaths(agentLoopId as ID);
      res.json(successResponse(analysis, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:agentLoopId/metrics", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<IterationAnalysisAdapter>("iteration-analysis");
      const agentLoopId = getSafeParam(req.params["agentLoopId"]);
      if (!agentLoopId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const iterationIndex = req.query["iteration"] ? parseInt(req.query["iteration"] as string, 10) : undefined;
      const metrics = await adapter.getIterationMetrics(agentLoopId as ID, iterationIndex);
      res.json(successResponse(metrics, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}