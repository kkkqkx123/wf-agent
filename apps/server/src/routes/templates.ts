/**
 * Template Routes
 * REST API endpoints for template management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { TemplateAdapter } from "../adapters/template-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createTemplateRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  // Node templates
  router.get("/nodes", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.listNodeTemplates({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/nodes/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Template ID is required")); return; }
      const result = await adapter.getNodeTemplate(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Trigger templates
  router.get("/triggers", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const offset = getIntParam(req.query["offset"] as string, 0);
      const limit = getIntParam(req.query["limit"] as string, 20);
      const result = await adapter.listTriggerTemplates({ offset, limit });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/triggers/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Template ID is required")); return; }
      const result = await adapter.getTriggerTemplate(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}