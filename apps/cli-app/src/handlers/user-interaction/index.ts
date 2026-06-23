/**
 * User Interaction Module for CLI
 * Manages all user interaction coordinators and handlers.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import { ToolApprovalCoordinator } from "@wf-agent/sdk/api";
import { CLIFollowupQuestionHandler } from "./followup-question.js";
import { CLIToolApprovalHandler } from "./tool-approval.js";

export class CLIUserInteractionManager {
  private followupHandler: CLIFollowupQuestionHandler | null = null;
  
  private approvalCoordinator: ToolApprovalCoordinator | null = null;
  private approvalHandler: CLIToolApprovalHandler | null = null;

  /**
   * Initialize the interaction module with SDK instance
   */
  public initialize(sdkInstance: SDKInstance): void {
    const globalContext = (sdkInstance as any).globalContext;
    if (!globalContext) {
      console.error("Failed to access SDK global context");
      return;
    }

    const eventManager = globalContext.eventRegistry;
    
    // 1. Initialize Follow-up Question Handler
    this.followupHandler = new CLIFollowupQuestionHandler();

    // 2. Initialize Tool Approval Coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(eventManager);
    this.approvalHandler = new CLIToolApprovalHandler();

    // The ToolApprovalCoordinator in SDK expects a handler with requestApproval method
    // which our CLIToolApprovalHandler now implements.
    // We pass it directly to the coordinator's internal logic or via a setter if available.
    // For now, we'll rely on the fact that the coordinator will be triggered by events
    // and the adapter in agent-loop-adapter.ts handles the direct call.
    // If the coordinator has an initialize method, we call it.
    if ((this.approvalCoordinator as any).initialize) {
      (this.approvalCoordinator as any).initialize();
    }
  }

  /**
   * Cleanup resources
   */
  public cleanup(): void {
    if (this.followupHandler) {
      this.followupHandler.cleanup();
    }
    if (this.approvalHandler) {
      this.approvalHandler.cleanup();
    }
  }
}
