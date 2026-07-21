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
import { NotFoundError, type ID } from "@wf-agent/types";
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
   */
  protected async executeInternal(): Promise<void> {
    const registry = this.dependencies.getAgentLoopRegistry();
    const agentLoop = await registry.get(this.params.agentLoopId);

    if (!agentLoop) {
      throw new NotFoundError(
        `Agent Loop not found: ${this.params.agentLoopId}`,
        "agent_loop",
        this.params.agentLoopId,
      );
    }

    // Get triggers from config
    const triggers = agentLoop.config.triggers || [];
    const trigger = triggers.find((t: any) => t.id === this.params.triggerId);

    if (!trigger) {
      throw new NotFoundError(
        `Trigger not found: ${this.params.triggerId}`,
        "trigger",
        this.params.triggerId,
      );
    }

    // Disable the trigger
    trigger.enabled = false;
  }
}
