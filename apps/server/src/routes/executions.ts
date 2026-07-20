/**
 * Execution Routes
 *
 * REST API endpoints for workflow execution management:
 * - POST /executions - Execute a workflow
 * - GET /executions/:id - Get execution status
 * - POST /executions/:id/pause - Pause execution
 * - POST /executions/:id/resume - Resume execution
 * - POST /executions/:id/stop - Stop execution
 * - GET /executions/:id/logs - Get execution logs
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { ExecutionService } from "../services/execution-service.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createExecutionRoutes(
  container: ServerDependencyContainer
): Router {
  const router = Router();

  /**
   * List all executions
   * GET /executions?workflowId=&status=
   */
  router.get("/", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const workflowId = getSafeParam(req.query["workflowId"] as string);
      const status = getSafeParam(req.query["status"] as string);
      const filter: { workflowId?: string; status?: string } = {};
      if (workflowId) filter.workflowId = workflowId;
      if (status) filter.status = status;
      const result = await service.list(filter);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Execute a workflow
   * POST /executions
   * Body: { workflowId, input?, mode? }
   */
  router.post("/", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const { workflowId, input, mode } = req.body;

      if (!workflowId) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Workflow ID is required",
          { required: ["workflowId"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const executionId = await service.execute(workflowId, input, mode);
      res.status(201).json(
        successResponse(
          { executionId, workflowId, mode: mode || "detached", status: "running" },
          { path: req.path, method: req.method }
        )
      );
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get execution status
   * GET /executions/:id
   */
  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Execution ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const result = await service.getStatus(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Pause execution
   * POST /executions/:id/pause
   */
  router.post("/:id/pause", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Execution ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      await service.pause(id);
      res.json(successResponse({ executionId: id, status: "paused" }, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Resume execution
   * POST /executions/:id/resume
   */
  router.post("/:id/resume", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Execution ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      await service.resume(id);
      res.json(successResponse({ executionId: id, status: "running" }, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Stop execution
   * POST /executions/:id/stop
   */
  router.post("/:id/stop", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Execution ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      await service.stop(id);
      res.json(successResponse({ executionId: id, status: "cancelled" }, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  /**
   * Get execution logs
   * GET /executions/:id/logs?offset=0&limit=100
   */
  router.get("/:id/logs", async (req: Request, res: Response) => {
    try {
      const service = container.getService<ExecutionService>("execution");
      const id = getSafeParam(req.params["id"]);

      if (!id) {
        const response = errorResponse(
          "VALIDATION_ERROR",
          "Execution ID is required",
          { required: ["id"] },
          { path: req.path, method: req.method }
        );
        res.status(400).json(response);
        return;
      }

      const offset = getIntParam(req.query["offset"], 0);
      const limit = getIntParam(req.query["limit"], 100);

      const logs = await service.getLogs(id, { offset, limit });
      res.json(successResponse(logs, { path: req.path, method: req.method }));
    } catch (error) {
      const response = mapErrorToResponse(error, req.path, req.method);
      res.status(getHttpStatus(response.error?.code || "INTERNAL_ERROR")).json(response);
    }
  });

  return router;
}
