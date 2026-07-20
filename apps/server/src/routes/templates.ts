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

  // Instantiate a node template
  router.post("/nodes/:id/instantiate", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Template ID is required")); return; }
      const result = await adapter.instantiateNodeTemplate(id, req.body?.params);
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Register node template from file
  router.post("/nodes/import", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const { filePath } = req.body;
      if (!filePath) { res.status(400).json(errorResponse("VALIDATION_ERROR", "filePath is required")); return; }
      const result = await adapter.registerNodeTemplateFromFile(filePath);
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Batch register node templates
  router.post("/nodes/import-batch", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const { configDir, recursive, filePattern } = req.body;
      if (!configDir) { res.status(400).json(errorResponse("VALIDATION_ERROR", "configDir is required")); return; }
      const result = await adapter.registerNodeTemplatesFromDirectory({
        configDir,
        recursive,
        filePattern: filePattern ? new RegExp(filePattern) : undefined,
      });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Delete node template
  router.delete("/nodes/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Template ID is required")); return; }
      await adapter.deleteNodeTemplate(id);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Register trigger template from file
  router.post("/triggers/import", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const { filePath } = req.body;
      if (!filePath) { res.status(400).json(errorResponse("VALIDATION_ERROR", "filePath is required")); return; }
      const result = await adapter.registerTriggerTemplateFromFile(filePath);
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Batch register trigger templates
  router.post("/triggers/import-batch", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const { configDir, recursive, filePattern } = req.body;
      if (!configDir) { res.status(400).json(errorResponse("VALIDATION_ERROR", "configDir is required")); return; }
      const result = await adapter.registerTriggerTemplatesFromDirectory({
        configDir,
        recursive,
        filePattern: filePattern ? new RegExp(filePattern) : undefined,
      });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  // Delete trigger template
  router.delete("/triggers/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<TemplateAdapter>("template");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Template ID is required")); return; }
      await adapter.deleteTriggerTemplate(id);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}