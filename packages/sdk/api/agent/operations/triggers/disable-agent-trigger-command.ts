/**
 * DisableAgentTriggerCommand - Disable Agent Trigger Command
 *
 * Category: Management
 * Disables a trigger for an agent loop execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import { validateAgentTriggerParams } from "../../../shared/operations/validators/agent-validators.js";
import type { ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Disable agent trigger command parameters
 */
export interface DisableAgentTriggerParams {
  /** Agent loop ID */
  agentLoopId: ID;
  /** Trigger ID to disable */
  triggerId: string;
}

/**
 * DisableAgentTriggerCommand - Disable Agent Trigger
 */
export class DisableAgentTriggerCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: DisableAgentTriggerParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "DisableAgentTriggerCommand",
      description: "Disable a trigger for an agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  /**
   * Verify command parameters
   */
  validate(): CommandValidationResult {
    return validateAgentTriggerParams(this.params.agentLoopId, this.params.triggerId);
  }

  /**
 * Execute command
 * Delegates to AgentLoopCoordinator for proper lifecycle management
 */
  protected async executeInternal(): Promise<void> {
    const coordinator = this.dependencies.getAgentLoopCoordinator();
    await coordinator.disableTrigger(this.params.agentLoopId, this.params.triggerId);
  }
}
