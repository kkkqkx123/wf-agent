/**
 * Interaction Service
 *
 * Manages user interactions during workflow execution, including
 * tool approval requests and follow-up questions.
 * Provides a polling/push mechanism for pending interactions.
 */

import { EventManager } from "./event-manager.js";

export interface ApprovalRequest {
  id: string;
  executionId: string;
  toolName: string;
  toolInput: Record<string, any>;
  context: string;
  status: "pending" | "approved" | "rejected";
  createdAt: string;
  resolvedAt?: string;
}

export interface FollowupQuestion {
  id: string;
  executionId: string;
  question: string;
  context: string;
  status: "pending" | "answered";
  answer?: string;
  createdAt: string;
  answeredAt?: string;
}

export class InteractionService {
  private pendingApprovals: Map<string, ApprovalRequest> = new Map();
  private pendingQuestions: Map<string, FollowupQuestion> = new Map();
  private eventManager = EventManager.getInstance();
  private static instance: InteractionService;

  /**
   * Get singleton instance
   */
  static getInstance(): InteractionService {
    if (!InteractionService.instance) {
      InteractionService.instance = new InteractionService();
    }
    return InteractionService.instance;
  }

  /**
   * Request tool approval
   */
  async requestApproval(
    executionId: string,
    toolName: string,
    toolInput: Record<string, any>,
    context: string
  ): Promise<ApprovalRequest> {
    const request: ApprovalRequest = {
      id: `approval_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      executionId,
      toolName,
      toolInput,
      context,
      status: "pending",
      createdAt: new Date().toISOString(),
    };

    this.pendingApprovals.set(request.id, request);

    this.eventManager.emit({
      type: "status",
      executionId,
      timestamp: new Date().toISOString(),
      data: {
        type: "approval_requested",
        approvalId: request.id,
        toolName,
        context,
      },
    });

    return request;
  }

  /**
   * Submit approval decision
   */
  async submitApproval(approvalId: string, approved: boolean): Promise<ApprovalRequest> {
    const request = this.pendingApprovals.get(approvalId);
    if (!request) {
      throw new Error(`Approval request not found: ${approvalId}`);
    }
    if (request.status !== "pending") {
      throw new Error(`Approval request ${approvalId} is already ${request.status}`);
    }

    request.status = approved ? "approved" : "rejected";
    request.resolvedAt = new Date().toISOString();

    this.eventManager.emit({
      type: "status",
      executionId: request.executionId,
      timestamp: new Date().toISOString(),
      data: {
        type: "approval_resolved",
        approvalId,
        status: request.status,
      },
    });

    return request;
  }

  /**
   * Ask a follow-up question
   */
  async askQuestion(
    executionId: string,
    question: string,
    context: string
  ): Promise<FollowupQuestion> {
    const q: FollowupQuestion = {
      id: `question_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      executionId,
      question,
      context,
      status: "pending",
      createdAt: new Date().toISOString(),
    };

    this.pendingQuestions.set(q.id, q);

    this.eventManager.emit({
      type: "status",
      executionId,
      timestamp: new Date().toISOString(),
      data: {
        type: "question_asked",
        questionId: q.id,
        question,
      },
    });

    return q;
  }

  /**
   * Answer a follow-up question
   */
  async answerQuestion(questionId: string, answer: string): Promise<FollowupQuestion> {
    const q = this.pendingQuestions.get(questionId);
    if (!q) {
      throw new Error(`Question not found: ${questionId}`);
    }
    if (q.status !== "pending") {
      throw new Error(`Question ${questionId} is already answered`);
    }

    q.status = "answered";
    q.answer = answer;
    q.answeredAt = new Date().toISOString();

    this.eventManager.emit({
      type: "status",
      executionId: q.executionId,
      timestamp: new Date().toISOString(),
      data: {
        type: "question_answered",
        questionId,
        answer,
      },
    });

    return q;
  }

  /**
   * List pending approvals for an execution
   */
  listPendingApprovals(executionId?: string): ApprovalRequest[] {
    const all = Array.from(this.pendingApprovals.values());
    if (executionId) {
      return all.filter((a) => a.executionId === executionId && a.status === "pending");
    }
    return all.filter((a) => a.status === "pending");
  }

  /**
   * List pending questions for an execution
   */
  listPendingQuestions(executionId?: string): FollowupQuestion[] {
    const all = Array.from(this.pendingQuestions.values());
    if (executionId) {
      return all.filter((q) => q.executionId === executionId && q.status === "pending");
    }
    return all.filter((q) => q.status === "pending");
  }

  /**
   * Get approval by ID
   */
  getApproval(id: string): ApprovalRequest | undefined {
    return this.pendingApprovals.get(id);
  }

  /**
   * Get question by ID
   */
  getQuestion(id: string): FollowupQuestion | undefined {
    return this.pendingQuestions.get(id);
  }

  /**
   * Cleanup completed interactions for an execution
   */
  cleanupExecution(executionId: string): void {
    for (const [id, req] of this.pendingApprovals) {
      if (req.executionId === executionId && req.status !== "pending") {
        this.pendingApprovals.delete(id);
      }
    }
    for (const [id, q] of this.pendingQuestions) {
      if (q.executionId === executionId && q.status !== "pending") {
        this.pendingQuestions.delete(id);
      }
    }
  }

  /**
   * Cleanup all resources
   */
  cleanup(): void {
    this.pendingApprovals.clear();
    this.pendingQuestions.clear();
  }
}