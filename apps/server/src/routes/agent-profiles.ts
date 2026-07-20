/**
 * Agent Profile Routes
 * REST API endpoints for agent profile management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { AgentProfileAdapter } from "../adapters/agent-profile-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";

export function createAgentProfileRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentProfileAdapter>("agent-profile");
      const profiles = await adapter.listProfiles();
      res.json(successResponse(profiles, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentProfileAdapter>("agent-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Profile ID is required")); return; }
      const profile = await adapter.getProfile(id);
      res.json(successResponse(profile, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/register", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentProfileAdapter>("agent-profile");
      const { id, name, description } = req.body || {};
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Profile id is required")); return; }
      const profile = await adapter.registerFromMeta({ id, name, description });
      res.status(201).json(successResponse(profile, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<AgentProfileAdapter>("agent-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Profile ID is required")); return; }
      await adapter.deleteProfile(id);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}