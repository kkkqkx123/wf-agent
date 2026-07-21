/**
 * CancelAgentLoopCommand - Cancel Agent Loop Command
 *
 * Category: Management
 * Cancels a running or paused agent loop execution.
 * Delegates to AgentLoopCoordinator for proper lifecycle management,
 * following the same delegation pattern as Workflow commands.
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import { validateAgentLoopControlParams } from "../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../shared/types/command.js";
import type { ID } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * Cancel Agent Loop command parameters
 */
export interface CancelAgentLoopParams {
  /** Agent Loop ID to cancel */
  agentLoopId: ID;
  /** Optional reason for cancellation */
  reason?: string;
}

/**
 * Cancel Agent Loop Command
 * Delegates to AgentLoopCoordinator for lifecycle management.
 */
export class CancelAgentLoopCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: CancelAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "CancelAgentLoopCommand",
      description: "Cancel a running or paused agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const coordinator = this.dependencies.getAgentLoopCoordinator();
    await coordinator.cancelAgentLoop(this.params.agentLoopId);
  }

  validate(): CommandValidationResult {
    return validateAgentLoopControlParams(this.params.agentLoopId);
  }
}