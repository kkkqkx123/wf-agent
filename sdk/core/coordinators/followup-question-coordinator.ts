/**
 * Follow-up Question Coordinator
 * Handles the business logic for follow-up question interactions.
 * Similar to ToolApprovalCoordinator, this is a stateful, business-logic-focused component.
 */

import type { EventRegistry } from "../registry/event-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { 
  FollowupQuestionRequestData,
  FollowupQuestionResponseData 
} from "@wf-agent/types";

export class FollowupQuestionCoordinator {
  private eventManager: EventRegistry;
  private logger: ReturnType<typeof createContextualLogger>;
  private timeoutMs: number;
  private uiAdapter: ((data: FollowupQuestionRequestData) => Promise<FollowupQuestionResponseData>) | null = null;

  constructor(eventManager: EventRegistry, options?: { timeoutMs?: number }) {
    this.eventManager = eventManager;
    this.logger = createContextualLogger({ component: "FollowupQuestionCoordinator" });
    this.timeoutMs = options?.timeoutMs ?? 5 * 60 * 1000; // Default 5 minutes
  }

  /**
   * Register the UI adapter function that handles the actual user input
   */
  public registerUIAdapter(adapter: (data: FollowupQuestionRequestData) => Promise<FollowupQuestionResponseData>): void {
    this.uiAdapter = adapter;
  }

  /**
   * Initialize the coordinator by subscribing to relevant events
   */
  public initialize(): void {
    this.eventManager.on(
      "FOLLOWUP_QUESTION_REQUESTED" as any,
      this.handleFollowupQuestionRequest.bind(this),
    );
    this.logger.info("FollowupQuestionCoordinator initialized");
  }

  /**
   * Handle the follow-up question request event
   */
  private async handleFollowupQuestionRequest(event: any): Promise<void> {
    const executionId = event.executionId || "unknown";
    const nodeId = event.nodeId || "unknown";
    const requestData: FollowupQuestionRequestData = event.data || {};
    
    this.logger.info(`Handling follow-up question request for execution ${executionId}, node ${nodeId}`);

    if (!this.uiAdapter) {
      this.logger.error("No UI adapter registered for follow-up questions");
      this.emitFailure(executionId, nodeId, "No UI adapter available");
      return;
    }

    try {
      // Set up timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error("Follow-up question timed out")), this.timeoutMs);
      });

      // Race between UI adapter and timeout
      const responseData = await Promise.race([
        this.uiAdapter(requestData),
        timeoutPromise,
      ]);

      this.emitSuccess(executionId, nodeId, responseData);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.logger.error(`Failed to process follow-up question: ${message}`);
      this.emitFailure(executionId, nodeId, message);
    }
  }

  /**
   * Emit success response event
   */
  private emitSuccess(executionId: string, nodeId: string, responseData: FollowupQuestionResponseData): void {
    const successEvent = {
      type: "FOLLOWUP_QUESTION_RESPONSE",
      executionId,
      nodeId,
      timestamp: Date.now(),
      data: responseData,
    };
    this.eventManager.emit(successEvent as any);
    this.logger.info(`Emitted follow-up question response for execution ${executionId}`);
  }

  /**
   * Emit failure event
   */
  private emitFailure(executionId: string, nodeId: string, reason: string): void {
    const failureEvent = {
      type: "FOLLOWUP_QUESTION_RESPONSE",
      executionId,
      nodeId,
      timestamp: Date.now(),
      data: {
        answers: [],
        additionalInfo: undefined,
        error: reason,
      },
    };
    this.eventManager.emit(failureEvent as any);
    this.logger.warn(`Emitted follow-up question failure for execution ${executionId}: ${reason}`);
  }

  /**
   * Cleanup resources
   */
  public cleanup(): void {
    this.uiAdapter = null;
    this.logger.info("FollowupQuestionCoordinator cleaned up");
  }
}
