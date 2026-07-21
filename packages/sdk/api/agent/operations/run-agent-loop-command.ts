/**
 * RunAgentLoopCommand - Run Agent Loop Command
 *
 * Category: Execution
 * Executes an agent loop with configuration and optional parameters
 */

import {
  ExecutionCommand,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import { validateAgentLoopRunParams } from "../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../shared/types/command.js";
import type { AgentLoopRuntimeConfig, AgentLoopResult } from "@wf-agent/types";
import type { AgentLoopEntityOptions } from "../../../agent/execution/factories/agent-loop-factory.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { withExecutionTimeout } from "../../shared/utils/timeout-execution.js";

/**
 * Run Agent Loop Command Parameters
 */
export interface RunAgentLoopParams {
  /** Agent Loop configuration */
  config: AgentLoopRuntimeConfig;
  /** Implementation options */
  options?: AgentLoopEntityOptions;
  /** Optional execution timeout in milliseconds */
  timeoutMs?: number;
}

/**
 * Run Agent Loop Command
 * Executes an agent loop and returns the result
 */
export class RunAgentLoopCommand extends ExecutionCommand<AgentLoopResult> {
  constructor(
    private readonly params: RunAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "RunAgentLoopCommand",
      description: "Execute an agent loop with configuration",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<AgentLoopResult> {
    const logger = createContextualLogger({
      component: "RunAgentLoopCommand",
      commandName: "RunAgentLoopCommand",
    });

    const startTime = Date.now();
    const maxIterations = this.params.config?.maxIterations ?? 10;
    const estimatedDefaultTimeout = maxIterations * 30000; // 30s per iteration

    logger.info("Command execution started", {
      maxIterations,
      profileId: this.params.config?.profileId,
      timeoutMs: this.params.timeoutMs ?? estimatedDefaultTimeout,
    });

    try {
      const coordinator = this.dependencies.getAgentLoopCoordinator();
      const result = await withExecutionTimeout(
        coordinator.execute(this.params.config, this.params.options),
        this.params.timeoutMs ?? estimatedDefaultTimeout,
        "Agent Loop Execution"
      );

      const duration = Date.now() - startTime;
      logger.info("Command execution completed successfully", undefined, {
        iterations: result.iterations,
        success: result.success,
        toolCallCount: result.toolCallCount,
        duration,
      });

      return result;
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMsg = error instanceof Error ? error.message : String(error);

      if (errorMsg.includes("timeout")) {
        logger.warn("Command execution timeout", undefined, {
          duration,
          timeoutMs: this.params.timeoutMs ?? estimatedDefaultTimeout,
        });
      } else {
        logger.error("Command execution failed", undefined, { duration }, error as Error);
      }

      throw error;
    }
  }

  validate(): CommandValidationResult {
    return validateAgentLoopRunParams(this.params.config);
  }
}
