/**
 * ResumeAgentLoopCommand - Resume Agent Loop Command
 *
 * Category: Management
 * Resumes a paused agent loop execution.
 * Delegates to AgentLoopCoordinator for proper lifecycle management,
 * following the same delegation pattern as Workflow commands.
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import { validateAgentLoopControlParams } from "../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../shared/types/command.js";
import type { ID, AgentLoopResult } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * Resume Agent Loop command parameters
 */
export interface ResumeAgentLoopParams {
  /** Agent Loop ID to resume */
  agentLoopId: ID;
}

/**
 * Resume Agent Loop Command
 * Delegates to AgentLoopCoordinator for lifecycle management.
 */
export class ResumeAgentLoopCommand extends ManagementCommand<AgentLoopResult> {
  constructor(
    private readonly params: ResumeAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ResumeAgentLoopCommand",
      description: "Resume a paused agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<AgentLoopResult> {
    const coordinator = this.dependencies.getAgentLoopCoordinator();
    const result = await coordinator.resumeAgentLoop(this.params.agentLoopId);
    return result;
  }

  validate(): CommandValidationResult {
    return validateAgentLoopControlParams(this.params.agentLoopId);
  }
}