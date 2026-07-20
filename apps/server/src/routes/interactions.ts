/**
 * User Interaction Routes
 *
 * REST API endpoints for user interactions during workflow execution:
 * - Tool approval requests and responses
 * - Follow-up question handling
 */

import { Router, type Request, type Response } from "express";
import { InteractionService } from "../services/interaction-service.js";
import { successResponse, errorResponse, mapErrorToResponse, getHttpStatus } from "../utils/api-response.js";
import { getSafeParam } from "./route-helpers.js";

export function createInteractionRoutes(): Router {
  const router = Router();
  const service = InteractionService.getInstance();

  /**
   * List pending approvals
   * GET /interactions/approvals?executionId=
   */
  router.get("/approvals", async (req: Request, res: Response) => {
    try {
      const executionId = getSafeParam(req.query["executionId"] as string);
      const approvals = service.listPendingApprovals(executionId);
      res.json(successResponse(approvals, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Get approval status
   * GET /interactions/approvals/:id
   */
  router.get("/approvals/:id", async (req: Request, res: Response) => {
    try {
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Approval ID is required")); return; }
      const approval = service.getApproval(id);
      if (!approval) { res.status(404).json(errorResponse("NOT_FOUND", "Approval not found")); return; }
      res.json(successResponse(approval, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Submit approval decision
   * POST /interactions/approvals/:id/respond
   * Body: { approved: boolean }
   */
  router.post("/approvals/:id/respond", async (req: Request, res: Response) => {
    try {
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Approval ID is required")); return; }
      const { approved } = req.body;
      if (approved === undefined) { res.status(400).json(errorResponse("VALIDATION_ERROR", "approved is required")); return; }
      const result = await service.submitApproval(id, approved);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * List pending questions
   * GET /interactions/questions?executionId=
   */
  router.get("/questions", async (req: Request, res: Response) => {
    try {
      const executionId = getSafeParam(req.query["executionId"] as string);
      const questions = service.listPendingQuestions(executionId);
      res.json(successResponse(questions, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Get question details
   * GET /interactions/questions/:id
   */
  router.get("/questions/:id", async (req: Request, res: Response) => {
    try {
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Question ID is required")); return; }
      const question = service.getQuestion(id);
      if (!question) { res.status(404).json(errorResponse("NOT_FOUND", "Question not found")); return; }
      res.json(successResponse(question, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  /**
   * Answer a question
   * POST /interactions/questions/:id/answer
   * Body: { answer: string }
   */
  router.post("/questions/:id/answer", async (req: Request, res: Response) => {
    try {
      const id = getSafeParam(req.params["id"]);
      if (!id) { res.status(400).json(errorResponse("VALIDATION_ERROR", "Question ID is required")); return; }
      const { answer } = req.body;
      if (!answer) { res.status(400).json(errorResponse("VALIDATION_ERROR", "answer is required")); return; }
      const result = await service.answerQuestion(id, answer);
      res.json(successResponse(result, { path: req.path, method: req.method }));
    } catch (error) {
      res.status(getHttpStatus("INTERNAL_ERROR")).json(mapErrorToResponse(error, req.path, req.method));
    }
  });

  return router;
}