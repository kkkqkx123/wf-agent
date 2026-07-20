/**
 * LLM Profile Routes
 * REST API endpoints for LLM profile management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { LLMProfileAdapter } from "../adapters/llm-profile-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createLLMProfileRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.list({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "LLM Profile ID is required")); return; }
      const result = await adapter.get(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const result = await adapter.create(req.body || {});
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.put("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "LLM Profile ID is required")); return; }
      const result = await adapter.update(id, req.body || {});
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "LLM Profile ID is required")); return; }
      await adapter.delete(id);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.post("/:id/set-default", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<LLMProfileAdapter>("llm-profile");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "LLM Profile ID is required")); return; }
      await adapter.setDefault(id);
      res.json(successResponse({ id, isDefault: true }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}