/**
 * Workflow Version Routes
 * REST API endpoints for workflow version management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { WorkflowVersionAdapter } from "../adapters/workflow-version-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";
import type { ID } from "@wf-agent/types";

export function createWorkflowVersionRoutes(
  container: ServerDependencyContainer
): Router {
  const router = Router();

  /**
   * List all versions of a workflow
   * GET /versions/:workflowId
   */
  router.get("/:workflowId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowVersionAdapter>("workflow-version");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const versions = await adapter.listVersions(workflowId as ID);
      res.json(successResponse(versions, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get specific version
   * GET /versions/:workflowId/:version
   */
  router.get("/:workflowId/:version", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowVersionAdapter>("workflow-version");
      const workflowId = getSafeParam(req.params["workflowId"]);
      const version = getSafeParam(req.params["version"]);
      if (!workflowId || !version) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID and version are required"));
        return;
      }
      const result = await adapter.getVersion(workflowId as ID, version);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Compare two versions
   * GET /versions/:workflowId/diff?from=v1&to=v2
   */
  router.get("/:workflowId/diff", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowVersionAdapter>("workflow-version");
      const workflowId = getSafeParam(req.params["workflowId"]);
      const from = getSafeParam(req.query["from"] as string);
      const to = getSafeParam(req.query["to"] as string);
      if (!workflowId || !from || !to) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID, from, and to are required"));
        return;
      }
      const result = await adapter.getDiff(workflowId as ID, from, to);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get changelog
   * GET /versions/:workflowId/changelog
   */
  router.get("/:workflowId/changelog", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowVersionAdapter>("workflow-version");
      const workflowId = getSafeParam(req.params["workflowId"]);
      if (!workflowId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Workflow ID is required"));
        return;
      }
      const changelog = await adapter.getChangeLog(workflowId as ID);
      res.json(successResponse(changelog, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  return router;
}