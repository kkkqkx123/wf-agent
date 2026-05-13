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
  private initialized: boolean = false;
  private unsubscribe?: () => void;

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
    if (this.initialized) {
      this.logger.warn("FollowupQuestionCoordinator already initialized, skipping");
      return;
    }

    this.unsubscribe = this.eventManager.on(
      "FOLLOWUP_QUESTION_REQUESTED" as any,
      this.handleFollowupQuestionRequest.bind(this),
    );
    this.initialized = true;
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
      await this.emitFailure(executionId, nodeId, "No UI adapter available");
      return;
    }

    try {
      // Set up timeout with proper cleanup
      let timeoutId: NodeJS.Timeout | undefined;
      const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error("Follow-up question timed out")), this.timeoutMs);
      });

      // Race between UI adapter and timeout
      const responseData = await Promise.race([
        this.uiAdapter(requestData),
        timeoutPromise,
      ]);

      // Clear timeout if completed successfully
      if (timeoutId) {
        clearTimeout(timeoutId);
      }

      await this.emitSuccess(executionId, nodeId, responseData);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.logger.error(`Failed to process follow-up question: ${message}`);
      await this.emitFailure(executionId, nodeId, message);
    }
  }

  /**
   * Emit success response event
   */
  private async emitSuccess(executionId: string, nodeId: string, responseData: FollowupQuestionResponseData): Promise<void> {
    const successEvent = {
      type: "FOLLOWUP_QUESTION_RESPONSE",
      executionId,
      nodeId,
      timestamp: Date.now(),
      data: responseData,
    };
    await this.eventManager.emit(successEvent as any);
    this.logger.info(`Emitted follow-up question response for execution ${executionId}`);
  }

  /**
   * Emit failure event
   */
  private async emitFailure(executionId: string, nodeId: string, reason: string): Promise<void> {
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
    await this.eventManager.emit(failureEvent as any);
    this.logger.warn(`Emitted follow-up question failure for execution ${executionId}: ${reason}`);
  }

  /**
   * Cleanup resources
   */
  public cleanup(): void {
    // Unsubscribe from events
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = undefined;
    }
    
    this.uiAdapter = null;
    this.initialized = false;
    this.logger.info("FollowupQuestionCoordinator cleaned up");
  }
}
