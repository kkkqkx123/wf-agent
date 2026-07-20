/**
 * Script Routes
 * REST API endpoints for script management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { ScriptAdapter } from "../adapters/script-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createScriptRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ScriptAdapter>("script");
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
      const adapter = container.getAdapter<ScriptAdapter>("script");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Script ID is required")); return; }
      const result = await adapter.get(id);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Register script from file
   * POST /scripts/import
   * Body: { filePath }
   */
  router.post("/import", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ScriptAdapter>("script");
      const { filePath } = req.body;
      if (!filePath) { res.status(400).json(errorResponse("VALIDATION_ERROR", "filePath is required")); return; }
      const result = await adapter.registerFromFile(filePath);
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Batch register scripts from directory
   * POST /scripts/import-batch
   * Body: { configDir, recursive?, filePattern? }
   */
  router.post("/import-batch", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ScriptAdapter>("script");
      const { configDir, recursive, filePattern } = req.body;
      if (!configDir) { res.status(400).json(errorResponse("VALIDATION_ERROR", "configDir is required")); return; }
      const result = await adapter.registerFromDirectory({
        configDir,
        recursive,
        filePattern: filePattern ? new RegExp(filePattern) : undefined,
      });
      res.status(201).json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Delete script
   * DELETE /scripts/:id
   */
  router.delete("/:id", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<ScriptAdapter>("script");
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Script ID is required")); return; }
      await adapter.delete(id);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}