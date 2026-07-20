/**
 * Workflow Graph Routes
 * REST API endpoints for workflow graph structure analysis.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { WorkflowGraphAdapter } from "../adapters/workflow-graph-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";
import type { ID } from "@wf-agent/types";

export function createWorkflowGraphRoutes(
  container: ServerDependencyContainer
): Router {
  const router = Router();

  /**
   * Get workflow graph summary
   * GET /graph/:workflowId
   */
  router.get("/:workflowId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowGraphAdapter>("workflow-graph");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const summary = await adapter.getGraphSummary(workflowId as ID);
      res.json(successResponse(summary, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Analyze workflow graph
   * GET /graph/:workflowId/analyze
   */
  router.get("/:workflowId/analyze", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowGraphAdapter>("workflow-graph");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const analysis = await adapter.analyzeGraph(workflowId as ID);
      res.json(successResponse(analysis, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get workflow graph nodes
   * GET /graph/:workflowId/nodes?type=
   */
  router.get("/:workflowId/nodes", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowGraphAdapter>("workflow-graph");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const nodeType = getSafeParam(req.query["type"] as string);
      const nodes = await adapter.getNodes(workflowId as ID, nodeType);
      res.json(successResponse(nodes, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get workflow graph statistics
   * GET /graph/:workflowId/stats
   */
  router.get("/:workflowId/stats", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowGraphAdapter>("workflow-graph");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const [nodeStats, edgeStats] = await Promise.all([
        adapter.getNodeStats(workflowId as ID),
        adapter.getEdgeStats(workflowId as ID),
      ]);
      res.json(successResponse({ nodes: nodeStats, edges: edgeStats }, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  return router;
}