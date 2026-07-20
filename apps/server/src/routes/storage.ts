/**
 * Storage Routes
 * REST API endpoints for storage diagnostics.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { StorageDiagnosticsAdapter } from "../adapters/storage-diagnostics-adapter.js";
import { successResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";

export function createStorageRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/diagnose", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<StorageDiagnosticsAdapter>("storage");
      const result = await adapter.diagnose();
      res.json(successResponse(result, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  router.get("/health", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<StorageDiagnosticsAdapter>("storage");
      const result = await adapter.getHealth();
      res.json(successResponse(result, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  router.get("/stats", async (_req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<StorageDiagnosticsAdapter>("storage");
      const result = await adapter.getItemCounts();
      res.json(successResponse(result, { path: _req.path, method: _req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, _req.path, _req.method));
    }
  });

  return router;
}