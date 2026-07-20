/**
 * Agent Loop Routes
 * REST API endpoints for agent loop management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { AgentLoopAdapter } from "../adapters/agent-loop-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";
import type { ID } from "@wf-agent/types";

export function createAgentLoopRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.list({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Run a new agent loop
   * POST /agent-loops/run
   * Body: { name, config, ... }
   */
  router.post("/run", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const config = req.body;
      if (!config) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop configuration is required")); return; }
      const result = await adapter.run(config);
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const result = await adapter.get(id as ID);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Stop an agent loop
   * POST /agent-loops/:id/stop
   */
  router.post("/:id/stop", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      await adapter.stop(id as ID);
      res.json(successResponse({ id, status: "stopped" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Create a checkpoint for an agent loop
   * POST /agent-loops/:id/checkpoint
   */
  router.post("/:id/checkpoint", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const name = req.body?.name as string | undefined;
      const checkpoint = await adapter.createCheckpoint(id as ID, name);
      res.status(201).json(successResponse(checkpoint, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * List checkpoints for an agent loop
   * GET /agent-loops/:id/checkpoints
   */
  router.get("/:id/checkpoints", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      const checkpoints = await adapter.listCheckpoints(id as ID);
      res.json(successResponse(checkpoints, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Load a checkpoint for an agent loop
   * POST /agent-loops/checkpoints/:checkpointId/load
   */
  router.post("/checkpoints/:checkpointId/load", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const checkpointId = getSafeParam(req.params["checkpointId"]);
      if (!checkpointId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Checkpoint ID is required")); return; }
      await adapter.loadCheckpoint(checkpointId);
      res.json(successResponse({ checkpointId, status: "restored" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentLoopAdapter>("agent-loop");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Agent loop ID is required")); return; }
      await adapter.delete(id as ID);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}