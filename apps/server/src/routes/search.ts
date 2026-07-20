/**
 * Search Routes
 * REST API endpoint for cross-resource search.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { SearchAdapter } from "../adapters/search-adapter.js";
import { successResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam, getIntParam } from "./route-helpers.js";

export function createSearchRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<SearchAdapter>("search");
      const query = getSafeParam(req.query["q"] as string);
      const type = getSafeParam(req.query["type"] as string);
      const limit = getIntParam(req.query["limit"] as string, 20);
      if (!query) {
        res.status(400).json({ success: false, error: { code: "VALIDATION_ERROR", message: "Search query is required" } });
        return;
      }
      const result = await adapter.search({
        query,
        types: type ? [type] : undefined,
        limit,
      });
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}