/**
 * ExecuteThreadCommand - Execute Thread Command
 *
 * Responsibilities:
 * - Receives workflow ID and execution options as input
 * - Delegates to ThreadLifecycleCoordinator to execute thread
 * - Returns ThreadResult as execution result
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for ExecutionContext and ThreadLifecycleCoordinator
 * - Parameter validation is completed in validate() method
 * - Actual execution logic is implemented in executeInternal()
 *
 * Note:
 * - This command is only responsible for executing threads, not for registering workflows
 * - Workflow registration should be done through a separate API
 * - Thread is an execution instance of workflow, a new Thread is created each time
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { ThreadResult, ThreadOptions } from "@wf-agent/types";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Execute thread command parameters
 */
export interface ExecuteThreadParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execute the option */
  options?: ThreadOptions;
}

/**
 * Execute Thread Command
 *
 * Workflow:
 * 1. Validate parameters (workflowId is required)
 * 2. Execute thread using ThreadLifecycleCoordinator
 * 3. Return ThreadResult
 *
 * Execution Flow:
 * - ThreadLifecycleCoordinator.execute(workflowId, options)
 *   → ThreadBuilder.build(workflowId, options)  // Create ThreadContext
 *   → ThreadRegistry.register(threadContext)    // Register thread
 *   → ThreadLifecycleCoordinator.startThread(thread) // Start thread
 *   → ThreadExecutor.executeThread(threadContext) // Execute thread
 *   → ThreadLifecycleCoordinator.completeThread/failThread // Complete thread
 */
export class ExecuteThreadCommand extends BaseCommand<ThreadResult> {
  constructor(
    private readonly params: ExecuteThreadParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<ThreadResult> {
    // Obtain the ThreadLifecycleCoordinator through APIDependencyManager
    const lifecycleCoordinator = this.dependencies.getThreadLifecycleCoordinator();

    // Execute thread (delegated to ThreadLifecycleCoordinator)
    const result = await lifecycleCoordinator.execute(
      this.params.workflowId,
      this.params.options || {},
    );

    return result;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The workflowId must be provided.
    if (!this.params.workflowId || this.params.workflowId.trim().length === 0) {
      errors.push("The workflowId must be provided.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
