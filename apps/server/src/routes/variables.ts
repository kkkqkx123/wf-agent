/**
 * Variable Routes
 * REST API endpoints for workflow execution variable management.
 */

import { Router, type Request, type Response } from "express";
import type { ServerDependencyContainer } from "../services/container.js";
import type { VariableAdapter } from "../adapters/variable-adapter.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";

export function createVariableRoutes(container: ServerDependencyContainer): Router {
  const router = Router();

  router.get("/:executionId", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<VariableAdapter>("variable");
      const executionId = getSafeParam(req.params["executionId"]);
      if (!executionId) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID is required")); return; }
      const variables = await adapter.listVariables(executionId);
      res.json(successResponse(variables, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:executionId/:name", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<VariableAdapter>("variable");
      const executionId = getSafeParam(req.params["executionId"]);
      const name = getSafeParam(req.params["name"]);
      if (!executionId || !name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID and variable name are required")); return; }
      const value = await adapter.getVariable(executionId, name);
      res.json(successResponse({ name, value }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.put("/:executionId/:name", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<VariableAdapter>("variable");
      const executionId = getSafeParam(req.params["executionId"]);
      const name = getSafeParam(req.params["name"]);
      if (!executionId || !name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID and variable name are required")); return; }
      await adapter.setVariable(executionId, name, req.body?.value);
      res.json(successResponse({ name, executionId }, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.delete("/:executionId/:name", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<VariableAdapter>("variable");
      const executionId = getSafeParam(req.params["executionId"]);
      const name = getSafeParam(req.params["name"]);
      if (!executionId || !name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID and variable name are required")); return; }
      await adapter.deleteVariable(executionId, name);
      res.status(204).send();
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  router.get("/:executionId/:name/definition", async (req: Request, res: Response) => {
    try {
      const adapter = container.getAdapter<VariableAdapter>("variable");
      const executionId = getSafeParam(req.params["executionId"]);
      const name = getSafeParam(req.params["name"]);
      if (!executionId || !name) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Execution ID and variable name are required")); return; }
      const definition = await adapter.getVariableDefinition(executionId, name);
      res.json(successResponse(definition, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}