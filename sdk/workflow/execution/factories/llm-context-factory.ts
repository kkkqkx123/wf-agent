/**
 * LLM Context Factory
 * Responsible for creating various contexts for the LLM Execution Coordinator
 *
 * Design Principles:
 * - Centralized management of dependencies related to LLM execution
 * - Creation of appropriate contexts based on the scenario
 * - Simplification of the responsibilities of the LLMExecutionCoordinator
 * - Verification of the presence of necessary dependencies
 */

import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { InterruptionDetector } from "../interruption-detector.js";
import type { CheckpointState } from "../../checkpoint/checkpoint-state-manager.js";
import { ExecutionError } from "@wf-agent/types";

/**
 * Tool Approval Context
 */
export interface ToolApprovalContext {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  checkpointStateManager: CheckpointState;
  workflowRegistry?: WorkflowRegistry;
  graphRegistry?: WorkflowGraphRegistry;
}

/**
 * Interrupt detection context
 */
export interface InterruptionContext {
  threadRegistry?: WorkflowExecutionRegistry;
  interruptionDetector?: InterruptionDetector;
}

/**
 * Tool execution context
 */
export interface ToolExecutionContext {
  toolService: ToolRegistry;
  toolCallExecutor: ToolCallExecutor;
  eventManager: EventRegistry;
}

/**
 * LLM Call Context
 */
export interface LLMCallContext {
  llmExecutor: LLMExecutor;
  eventManager: EventRegistry;
  toolService: ToolRegistry;
}

/**
 * Tool Visibility Context
 */
export interface ToolVisibilityContext {
  toolContextStore?: unknown;
  toolVisibilityCoordinator?: unknown;
  toolService: ToolRegistry;
}

/**
 * LLM Context Factory Configuration
 */
export interface LLMContextFactoryConfig {
  // Core Dependencies (Essential)
  /** LLM Executor */
  llmExecutor: LLMExecutor;
  /** Tool Services */
  toolService: ToolRegistry;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Tool Call Executor */
  toolCallExecutor: ToolCallExecutor;

  // Contextual relevant (optional)
  /** Thread Registry */
  threadRegistry?: WorkflowExecutionRegistry;
  /** Interrupt Detector */
  interruptionDetector?: InterruptionDetector;
  /** Checkpoint State Manager */
  checkpointStateManager?: CheckpointState;
  /** Workflow Registry */
  workflowRegistry?: WorkflowRegistry;
  /** Image Registry */
  graphRegistry?: WorkflowGraphRegistry;
  /** Tool Context Store */
  toolContextStore?: unknown;
  /** Tool Visibility Coordinator */
  toolVisibilityCoordinator?: unknown;
}

/**
 * LLM Context Factory
 *
 * Responsibilities:
 * - Create the appropriate context based on the scenario.
 * - Centralize the management of dependencies required for LLM execution.
 * - Verify the presence of all necessary dependencies.
 */
export class LLMContextFactory {
  constructor(private config: LLMContextFactoryConfig) {}

  /**
   * Create a tool approval context
   *
   * @param threadId Thread ID
   * @param nodeId Node ID
   * @returns Tool approval context
   * @throws ExecutionError When required dependencies are missing
   */
  createToolApprovalContext(threadId: string, nodeId: string): ToolApprovalContext {
    if (!this.config.threadRegistry) {
      throw new ExecutionError(
        "WorkflowExecutionRegistry is required for tool approval context",
        nodeId,
        undefined,
        { threadId },
      );
    }

    if (!this.config.checkpointStateManager) {
      throw new ExecutionError(
        "CheckpointState is required for tool approval context",
        nodeId,
        undefined,
        { threadId },
      );
    }

    return {
      workflowExecutionRegistry: this.config.threadRegistry,
      checkpointStateManager: this.config.checkpointStateManager,
      workflowRegistry: this.config.workflowRegistry,
      workflowGraphRegistry: this.config.graphRegistry,
    };
  }

  /**
   * Create an interrupt detection context
   *
   * @returns Interrupt detection context
   */
  createInterruptionContext(): InterruptionContext {
    return {
      workflowExecutionRegistry: this.config.threadRegistry,
      interruptionDetector: this.config.interruptionDetector,
    };
  }

  /**
   * Create a tool execution context
   *
   * @returns Tool execution context
   */
  createToolExecutionContext(): ToolExecutionContext {
    return {
      toolService: this.config.toolService,
      toolCallExecutor: this.config.toolCallExecutor,
      eventManager: this.config.eventManager,
    };
  }

  /**
   * Create an LLM call context
   *
   * @returns LLM call context
   */
  createLLMCallContext(): LLMCallContext {
    return {
      llmExecutor: this.config.llmExecutor,
      eventManager: this.config.eventManager,
      toolService: this.config.toolService,
    };
  }

  /**
   * Create a tool visibility context
   *
   * @returns Tool visibility context
   */
  createToolVisibilityContext(): ToolVisibilityContext {
    return {
      toolContextStore: this.config.toolContextStore,
      toolVisibilityCoordinator: this.config.toolVisibilityCoordinator,
      toolService: this.config.toolService,
    };
  }

  /**
   * Check if the tool approval feature is supported
   *
   * @returns Whether it is supported
   */
  hasToolApprovalSupport(): boolean {
    return !!(this.config.threadRegistry && this.config.checkpointStateManager);
  }

  /**
   * Check if the interrupt detection feature is supported
   *
   * @returns Whether it is supported
   */
  hasInterruptionSupport(): boolean {
    return !!(this.config.interruptionDetector || this.config.threadRegistry);
  }

  /**
   * Get the thread registry
   */
  getThreadRegistry(): WorkflowExecutionRegistry | undefined {
    return this.config.threadRegistry;
  }

  /**
   * Obtain the event manager
   */
  getEventManager(): EventRegistry {
    return this.config.eventManager;
  }

  /**
   * Obtain tool services
   */
  getToolService(): ToolRegistry {
    return this.config.toolService;
  }

  /**
   * Obtain the LLM executor
   */
  getLLMExecutor(): LLMExecutor {
    return this.config.llmExecutor;
  }

  /**
   * Obtain the tool call executor
   */
  getToolCallExecutor(): ToolCallExecutor {
    return this.config.toolCallExecutor;
  }

  /**
   * Obtain the checkpoint status manager
   */
  getCheckpointStateManager(): CheckpointState | undefined {
    return this.config.checkpointStateManager;
  }

  /**
   * Get the workflow registry
   */
  getWorkflowRegistry(): WorkflowRegistry | undefined {
    return this.config.workflowRegistry;
  }

  /**
   * Obtain the image registry
   */
  getGraphRegistry(): WorkflowGraphRegistry | undefined {
    return this.config.graphRegistry;
  }

  /**
   * Obtain the tool context store
   */
  getToolContextStore(): unknown {
    return this.config.toolContextStore;
  }

  /**
   * Obtain the Tool Visibility Coordinator
   */
  getToolVisibilityCoordinator(): unknown {
    return this.config.toolVisibilityCoordinator;
  }
}
