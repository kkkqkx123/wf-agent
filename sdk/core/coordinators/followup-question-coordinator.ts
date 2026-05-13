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
  
  // Track active requests for cleanup (executionId -> unsubscribe function)
  private activeRequests: Map<string, () => void> = new Map();

  constructor(eventManager: EventRegistry, options?: { timeoutMs?: number }) {
    this.eventManager = eventManager;
    this.logger = createContextualLogger({ component: "FollowupQuestionCoordinator" });
    this.timeoutMs = options?.timeoutMs ?? 5 * 60 * 1000; // Default 5 minutes
  }

  /**
   * Initialize the coordinator (currently a no-op, kept for API consistency)
   */
  public initialize(): void {
    // No initialization needed currently
    this.logger.debug("FollowupQuestionCoordinator initialized");
  }

  /**
   * Register the UI adapter function that handles the actual user input
   */
  public registerUIAdapter(adapter: (data: FollowupQuestionRequestData) => Promise<FollowupQuestionResponseData>): void {
    this.uiAdapter = adapter;
  }

  /**
   * Handle a follow-up question request for a specific execution
   * This method should be called directly instead of using global event listeners
   */
  public async handleFollowupQuestionRequest(
    executionId: string,
    nodeId: string,
    requestData: FollowupQuestionRequestData
  ): Promise<FollowupQuestionResponseData> {
    this.logger.info(`Handling follow-up question request for execution ${executionId}, node ${nodeId}`);

    if (!this.uiAdapter) {
      this.logger.error("No UI adapter registered for follow-up questions");
      throw new Error("No UI adapter available");
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

      return responseData;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.logger.error(`Failed to process follow-up question: ${message}`);
      throw error;
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
   * Cleanup resources for a specific execution
   */
  public cleanupExecution(executionId: string): void {
    const unsubscribe = this.activeRequests.get(executionId);
    if (unsubscribe) {
      unsubscribe();
      this.activeRequests.delete(executionId);
      this.logger.debug(`Cleaned up follow-up question listener for execution ${executionId}`);
    }
  }

  /**
   * Cleanup all resources
   */
  public cleanup(): void {
    // Unsubscribe from all active requests
    for (const [executionId, unsubscribe] of this.activeRequests.entries()) {
      unsubscribe();
      this.logger.debug(`Cleaned up follow-up question listener for execution ${executionId}`);
    }
    this.activeRequests.clear();
    
    this.uiAdapter = null;
    this.logger.info("FollowupQuestionCoordinator cleaned up");
  }
}
