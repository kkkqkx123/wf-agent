/**
 * Workflow Routes
 *
 * REST API endpoints for workflow management:
 * - GET /workflows - List all workflows
 * - GET /workflows/:id - Get single workflow
 * - POST /workflows - Create workflow
 * - PUT /workflows/:id - Update workflow
 * - DELETE /workflows/:id - Delete workflow
 * - GET /workflows/:id/graph - Get workflow graph
 * - POST /workflows/import - Import workflow from file
 * - POST /workflows/import-batch - Batch import workflows from directory
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { WorkflowAdapter } from "../adapters/workflow-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createWorkflowRoutes(
  container: ServerDependencyContainer
): Router {
  const router = Router();

  /**
   * List all workflows
   * GET /workflows?offset=0&limit=20
   */
  router.get("/", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const offset = getIntParam(_req.query["offset"], 0);
      const limit = getIntParam(_req.query["limit"], 20);

      const result = await adapter.list({ offset, limit });
      res.json(successResponse(result, { path: _req.path, method: _req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, _req.path, _req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get single workflow
   * GET /workflows/:id
   */
  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.get(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Create new workflow
   * POST /workflows
   */
  router.post("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const { name, description, config } = req.body;

      if (!name) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow name is required",
          { required: ["name"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.create({
        name,
        description,
        config,
      });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Update workflow
   * PUT /workflows/:id
   */
  router.put("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const id = getSafeParam(req.params["id"]);
      const { name, description, config } = req.body;

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.update(id, {
        name,
        description,
        config,
      });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Delete workflow
   * DELETE /workflows/:id
   */
  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      await adapter.delete(id);
      res.status(204).send();
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get workflow graph
   * GET /workflows/:id/graph
   */
  router.get("/:id/graph", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.getGraph(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Import workflow from file
   * POST /workflows/import
   * Body: { filePath, parameters? }
   */
  router.post("/import", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const { filePath, parameters } = req.body;

      if (!filePath) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "filePath is required",
          { required: ["filePath"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.registerFromFile({ filePath, parameters });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Batch import workflows from directory
   * POST /workflows/import-batch
   * Body: { configDir, recursive?, filePattern?, parameters? }
   */
  router.post("/import-batch", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowAdapter>("workflow");
      const { configDir, recursive, filePattern, parameters } = req.body;

      if (!configDir) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "configDir is required",
          { required: ["configDir"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await adapter.registerFromDirectory({
        configDir,
        recursive,
        filePattern: filePattern ? new RegExp(filePattern) : undefined,
        parameters,
      });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  return router;
}
