/**
 * PauseAgentLoopCommand - Pause Agent Loop Command
 *
 * Category: Management
 * Pauses a running agent loop execution.
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
 * Pause Agent Loop command parameters
 */
export interface PauseAgentLoopParams {
  /** Agent Loop ID to pause */
  agentLoopId: ID;
}

/**
 * Pause Agent Loop Command
 * Delegates to AgentLoopCoordinator for lifecycle management.
 */
export class PauseAgentLoopCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: PauseAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "PauseAgentLoopCommand",
      description: "Pause a running agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const coordinator = this.dependencies.getAgentLoopCoordinator();
    await coordinator.pauseAgentLoop(this.params.agentLoopId);
  }

  validate(): CommandValidationResult {
    return validateAgentLoopControlParams(this.params.agentLoopId);
  }
}