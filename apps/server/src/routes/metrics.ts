/**
 * Metrics Routes
 * REST API endpoints for metrics querying.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { MetricsAdapter } from "../adapters/metrics-adapter.js";
import { successResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getIntParam } from "./route-helpers.js";

export function createMetricsRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/workflow", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MetricsAdapter>("metrics");
      const workflowId = req.query["workflowId"] as string | undefined;
      const result = await adapter.getWorkflowMetrics({ workflowId });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/node-templates", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MetricsAdapter>("metrics");
      const topN = getIntParam(req.query["topN"] as string, undefined);
      const result = await adapter.getNodeTemplateMetrics({ topN });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/agents", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MetricsAdapter>("metrics");
      const profileId = req.query["profileId"] as string | undefined;
      const result = await adapter.getAgentMetrics({ profileId });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/report", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MetricsAdapter>("metrics");
      const result = await adapter.getComprehensiveReport();
      res.json(successResponse(result, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  /**
   * Export metrics
   * GET /metrics/export?format=json|prometheus
   */
  router.get("/export", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<MetricsAdapter>("metrics");
      const format = (req.query["format"] as string) || "json";
      const result = await adapter.exportMetrics(format);
      if (format === "prometheus") {
        res.setHeader("Content-Type", "text/plain; charset=utf-8");
      }
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}