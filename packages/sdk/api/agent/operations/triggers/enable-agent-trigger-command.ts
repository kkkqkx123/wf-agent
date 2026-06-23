/**
 * EnableAgentTriggerCommand - Enable Agent Trigger Command
 *
 * Category: Management
 * Enables a trigger for an agent loop execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import { NotFoundError, type ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Enable agent trigger command parameters
 */
export interface EnableAgentTriggerParams {
  /** Agent loop ID */
  agentLoopId: ID;
  /** Trigger ID to enable */
  triggerId: string;
}

/**
 * EnableAgentTriggerCommand - Enable Agent Trigger
 */
export class EnableAgentTriggerCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: EnableAgentTriggerParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "EnableAgentTriggerCommand",
      description: "Enable a trigger for an agent loop",
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
    if (!this.params.agentLoopId) {
      return {
        valid: false,
        errors: ["Agent Loop ID is required"],
      };
    }
    if (!this.params.triggerId) {
      return {
        valid: false,
        errors: ["Trigger ID is required"],
      };
    }
    return { valid: true, errors: [] };
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

    // Enable the trigger
    trigger.enabled = true;
  }
}
