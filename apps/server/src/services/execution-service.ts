/**
 * Execution Service
 *
 * Manages workflow execution lifecycle including creation, monitoring, and control.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import {
  ExecuteWorkflowCommand,
  PauseWorkflowCommand,
  ResumeWorkflowCommand,
  CancelWorkflowCommand,
  isSuccess,
  getData,
  getError,
} from "@wf-agent/sdk/api";
import { getOutput } from "../utils/output.js";
import { EventManager, type ExecutionEvent } from "./event-manager.js";
import type { WorkflowExecutionMode, WorkflowExecution } from "@wf-agent/types";

/**
 * Workflow dispatch execution mode.
 * Reuses shared WorkflowExecutionMode from @wf-agent/types to eliminate
 * duplicate enum/type definitions between cli-app and server.
 */
export type ExecutionMode = WorkflowExecutionMode;

export enum ExecutionStatus {
  PENDING = "pending",
  RUNNING = "running",
  PAUSED = "paused",
  COMPLETED = "completed",
  FAILED = "failed",
  CANCELLED = "cancelled",
}

/**
 * Execution details interface
 */
export interface ExecutionDetails {
  id: string;
  workflowId: string;
  status: ExecutionStatus;
  progress?: number;
  currentNode?: string;
  startTime?: string;
  endTime?: string;
  error?: string;
  [key: string]: any;
}

/**
 * Log entry interface
 */
export interface LogEntry {
  timestamp: string;
  level: "debug" | "info" | "warn" | "error";
  message: string;
  context?: string;
  [key: string]: any;
}

/**
 * Execution Service
 * Handles workflow execution operations
 */
export class ExecutionService {
  private logger = getOutput();
  private activeExecutions = new Map<string, ExecutionDetails>();
  private eventManager: EventManager;
  private sdk: SDKInstance;

  constructor(sdk: SDKInstance, eventManager: EventManager) {
    this.sdk = sdk;
    this.eventManager = eventManager;
  }

  /**
   * List all executions
   */
  async list(filter?: { workflowId?: string; status?: string }): Promise<ExecutionDetails[]> {
    this.logger.debugLog("Listing all executions");

    try {
      // TODO: Implement SDK integration for full list
      // const executions = await this.sdk.executions.getAll(filter);
      const executions = Array.from(this.activeExecutions.values());
      let filtered = executions;
      if (filter?.workflowId) {
        filtered = filtered.filter((e) => e.workflowId === filter.workflowId);
      }
      if (filter?.status) {
        filtered = filtered.filter((e) => e.status === filter.status);
      }
      return filtered;
    } catch (error) {
      this.logger.errorLog(
        `Failed to list executions: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Execute a workflow
   */
  async execute(
    workflowId: string,
    input?: Record<string, any>,
    _mode?: ExecutionMode
  ): Promise<string> {
    this.logger.debugLog(`Executing workflow: ${workflowId}`);

    try {
      if (!workflowId || workflowId.trim().length === 0) {
        throw new Error("Workflow ID is required");
      }

      // Execute via SDK command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ExecuteWorkflowCommand({
        workflowId,
        options: { input: input as Record<string, unknown> | undefined },
      }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        const err = getError(result);
        throw new Error(err?.message || "Workflow execution failed");
      }

      const executionResult = getData(result);
      if (!executionResult) {
        throw new Error("Workflow execution result is null");
      }

      const executionId = executionResult.executionId;
      this.trackExecution(executionId, workflowId);
      this.logger.infoLog(`Workflow execution started: ${executionId}`);
      return executionId;
    } catch (error) {
      this.logger.errorLog(
        `Failed to execute workflow ${workflowId}: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Get execution status
   */
  async getStatus(executionId: string): Promise<ExecutionDetails> {
    this.logger.debugLog(`Getting status for execution: ${executionId}`);

    try {
      if (!executionId || executionId.trim().length === 0) {
        throw new Error("Execution ID is required");
      }

      // Check if tracked locally first
      const tracked = this.activeExecutions.get(executionId);
      if (tracked) {
        return tracked;
      }

      // Fallback: query from SDK storage
      const execution = await this.sdk.executions.get(executionId);
      if (!execution) {
        throw new Error(`Execution not found: ${executionId}`);
      }

      return this.toExecutionDetails(execution as WorkflowExecution);
    } catch (error) {
      this.logger.errorLog(
        `Failed to get execution status: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Pause an execution
   */
  async pause(executionId: string): Promise<void> {
    this.logger.debugLog(`Pausing execution: ${executionId}`);

    try {
      if (!executionId || executionId.trim().length === 0) {
        throw new Error("Execution ID is required");
      }

      // Execute via SDK command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new PauseWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        const err = getError(result);
        throw new Error(err?.message || "Failed to pause execution");
      }

      // Update local tracking
      const tracked = this.activeExecutions.get(executionId);
      if (tracked) {
        tracked.status = ExecutionStatus.PAUSED;
        this.emitEvent(executionId, "status", {
          status: ExecutionStatus.PAUSED,
          message: "Execution paused",
        });
      }
    } catch (error) {
      this.logger.errorLog(
        `Failed to pause execution: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Resume an execution
   */
  async resume(executionId: string): Promise<void> {
    this.logger.debugLog(`Resuming execution: ${executionId}`);

    try {
      if (!executionId || executionId.trim().length === 0) {
        throw new Error("Execution ID is required");
      }

      // Execute via SDK command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ResumeWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        const err = getError(result);
        throw new Error(err?.message || "Failed to resume execution");
      }

      // Update local tracking
      const tracked = this.activeExecutions.get(executionId);
      if (tracked) {
        tracked.status = ExecutionStatus.RUNNING;
        this.emitEvent(executionId, "status", {
          status: ExecutionStatus.RUNNING,
          message: "Execution resumed",
        });
      }
    } catch (error) {
      this.logger.errorLog(
        `Failed to resume execution: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Stop an execution
   */
  async stop(executionId: string): Promise<void> {
    this.logger.debugLog(`Stopping execution: ${executionId}`);

    try {
      if (!executionId || executionId.trim().length === 0) {
        throw new Error("Execution ID is required");
      }

      // Execute via SDK command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new CancelWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        const err = getError(result);
        throw new Error(err?.message || "Failed to stop execution");
      }

      // Update local tracking
      const tracked = this.activeExecutions.get(executionId);
      if (tracked) {
        tracked.status = ExecutionStatus.CANCELLED;
        tracked.endTime = new Date().toISOString();
        this.emitEvent(executionId, "status", {
          status: ExecutionStatus.CANCELLED,
          message: "Execution cancelled",
        });
        this.emitEvent(executionId, "complete", {
          status: ExecutionStatus.CANCELLED,
        });
      }
    } catch (error) {
      this.logger.errorLog(
        `Failed to stop execution: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Get execution logs
   */
  async getLogs(
    executionId: string,
    _pagination?: { offset?: number; limit?: number }
  ): Promise<LogEntry[]> {
    this.logger.debugLog(`Getting logs for execution: ${executionId}`);

    try {
      if (!executionId || executionId.trim().length === 0) {
        throw new Error("Execution ID is required");
      }

      // Note: SDK does not currently expose a dedicated log query API.
      // This is a placeholder for future implementation.
      // In the future, logs can be retrieved from the SDK's event system
      // or a dedicated log storage adapter.
      return [];
    } catch (error) {
      this.logger.errorLog(
        `Failed to get execution logs: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Track an execution locally
   * @internal
   */
  private trackExecution(executionId: string, workflowId: string): void {
    const execution: ExecutionDetails = {
      id: executionId,
      workflowId,
      status: ExecutionStatus.RUNNING,
      startTime: new Date().toISOString(),
    };

    this.activeExecutions.set(executionId, execution);

    // Emit execution started event
    this.emitEvent(executionId, "status", {
      status: ExecutionStatus.RUNNING,
      message: "Execution started",
    });

    // Auto-cleanup after 24 hours
    setTimeout(() => {
      this.activeExecutions.delete(executionId);
    }, 24 * 60 * 60 * 1000);
  }

  /**
   * Emit an execution event to subscribers
   * @internal
   */
  private emitEvent(
    executionId: string,
    type: ExecutionEvent["type"],
    data: Record<string, any>
  ): void {
    const event: ExecutionEvent = {
      type,
      executionId,
      timestamp: new Date().toISOString(),
      data,
    };

    this.eventManager.emit(event);
  }

  /**
   * Convert SDK WorkflowExecution to ExecutionDetails
   * @internal
   */
  private toExecutionDetails(execution: WorkflowExecution): ExecutionDetails {
    return {
      id: execution.id,
      workflowId: execution.workflowId,
      status: (execution as any).metadata?.status || ExecutionStatus.RUNNING,
      startTime: (execution as any).startTime || new Date().toISOString(),
      endTime: (execution as any).endTime,
      error: (execution as any).error,
      progress: (execution as any).progress,
      currentNode: (execution as any).currentNodeId,
    };
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    this.activeExecutions.clear();
    this.eventManager.clear();
    this.logger.debugLog("ExecutionService cleaned up");
  }
}