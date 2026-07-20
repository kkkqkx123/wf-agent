/**
 * Skill Routes
 * REST API endpoints for skill management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { SkillAdapter } from "../adapters/skill-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createSkillRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.list({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * List skill resources
   * GET /skills/resources
   */
  router.get("/resources", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const resources = await adapter.listResources();
      res.json(successResponse(resources, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  /**
   * Register a skill
   * POST /skills/register
   * Body: { name, metadata }
   */
  router.post("/register", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const { name, metadata } = req.body;
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      const result = await adapter.register(name, metadata || {});
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:name", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const name = getSafeParam(req.params["name"]);
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      const result = await adapter.get(name);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/:name/load", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const name = getSafeParam(req.params["name"]);
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      const content = await adapter.loadContent(name, req.body?.variables);
      res.json(successResponse({ name, content }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Unregister a skill
   * POST /skills/:name/unregister
   */
  router.post("/:name/unregister", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const name = getSafeParam(req.params["name"]);
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      await adapter.unregister(name);
      res.json(successResponse({ name, status: "unregistered" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Enable a skill
   * POST /skills/:name/enable
   */
  router.post("/:name/enable", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const name = getSafeParam(req.params["name"]);
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      await adapter.enable(name);
      res.json(successResponse({ name, status: "enabled" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Disable a skill
   * POST /skills/:name/disable
   */
  router.post("/:name/disable", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SkillAdapter>("skill");
      const name = getSafeParam(req.params["name"]);
      if (!name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Skill name is required")); return; }
      await adapter.disable(name);
      res.json(successResponse({ name, status: "disabled" }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}