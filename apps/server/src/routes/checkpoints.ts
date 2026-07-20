/**
 * Checkpoint Routes
 * REST API endpoints for workflow execution checkpoint management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { WorkflowExecutionCheckpointAdapter } from "../adapters/workflow-execution-checkpoint-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createCheckpointRoutes(
  container: ServerDependencyContainer
): Router {
  const router = Router();

  /**
   * List all checkpoints
   * GET /checkpoints?executionId=&offset=0&limit=20
   */
  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowExecutionCheckpointAdapter>("checkpoint");
      const filter: Record<string, unknown> = {};
      const executionId = getSafeParam(req.query["executionId"] as string);
      if (executionId) filter["executionId"] = executionId;
      filter["limit"] = getIntParam(req.query["limit"] as string, 50);

      const checkpoints = await adapter.listCheckpoints(filter);
      res.json(successResponse(checkpoints, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Create a checkpoint
   * POST /checkpoints/:executionId
   */
  router.post("/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowExecutionCheckpointAdapter>("checkpoint");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required"));
        return;
      }
      const name = req.body?.name as string | undefined;
      const checkpoint = await adapter.createCheckpoint(executionId, name);
      res.status(201).json(successResponse(checkpoint, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get checkpoint details
   * GET /checkpoints/:id
   */
  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowExecutionCheckpointAdapter>("checkpoint");
      const id = getSafeParam(req.params["id"]);
      if (!id) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Checkpoint ID is required"));
        return;
      }
      const checkpoint = await adapter.getCheckpoint(id);
      res.json(successResponse(checkpoint, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Restore a checkpoint
   * POST /checkpoints/:id/restore
   */
  router.post("/:id/restore", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowExecutionCheckpointAdapter>("checkpoint");
      const id = getSafeParam(req.params["id"]);
      if (!id) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Checkpoint ID is required"));
        return;
      }
      await adapter.loadCheckpoint(id);
      res.json(successResponse({ checkpointId: id, status: "restored" }, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Delete a checkpoint
   * DELETE /checkpoints/:id
   */
  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<WorkflowExecutionCheckpointAdapter>("checkpoint");
      const id = getSafeParam(req.params["id"]);
      if (!id) {
        res.status(400).json(errorResponse("VALIDATION_ERROR", "Checkpoint ID is required"));
        return;
      }
      await adapter.deleteCheckpoint(id);
      res.status(204).send();
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  return router;
}