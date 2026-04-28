/**
 * Task Executor
 * Responsible for executing workflow threads in isolated terminals.
 */

import { getSDK } from "@wf-agent/sdk";
import { randomUUID } from "crypto";
import { getOutput } from "../utils/output.js";
import type { TerminalSession, TaskExecutionResult, TaskStatus } from "./types.js";

const output = getOutput();

/**
 * Task Executor
 * Responsible for executing workflow threads in isolated terminals.
 */
export class TaskExecutor {
  /** Task Status Mapping Table */
  private tasks: Map<string, TaskStatus> = new Map();
  /** Task-Terminal Mapping Table */
  private taskTerminalMap: Map<string, string> = new Map();
  /** SDK instance */
  private sdk: ReturnType<typeof getSDK>;

  constructor() {
    this.sdk = getSDK();
  }

  /**
   * Execute task in isolated terminal
   * @param workflowId Workflow ID
   * @param input Input data
   * @param terminal Terminal session
   * @returns Task execution result
   */
  async executeInTerminal(
    workflowId: string,
    input: Record<string, unknown>,
    terminal: TerminalSession,
  ): Promise<TaskExecutionResult> {
    const taskId = randomUUID();

    // Initialize task status
    const taskStatus: TaskStatus = {
      taskId,
      status: "running",
      progress: 0,
      message: "Task has been initiated",
      lastUpdate: new Date(),
    };
    this.tasks.set(taskId, taskStatus);
    this.taskTerminalMap.set(taskId, terminal.id);

    // Constructing the command to execute
    const command = this.buildExecutionCommand(workflowId, input, taskId);

    try {
      // Execute the command in the terminal.
      terminal.pty.write(command + "\r");

      output.infoLog(`Task started: ${taskId} in terminal ${terminal.id}`);

      return {
        taskId,
        sessionId: terminal.id,
        status: "started",
        startTime: new Date(),
      };
    } catch (error) {
      // Update the task status to Failed.
      this.updateTaskStatus(taskId, {
        status: "failed",
        message: error instanceof Error ? error.message : String(error),
      });

      output.errorLog(
        `Task startup failed: ${taskId} - ${error instanceof Error ? error.message : String(error)}`,
      );
      throw error;
    }
  }

  /**
   * Monitor task status
   * @param taskId Task ID
   * @returns Task status
   */
  async monitorTask(taskId: string): Promise<TaskStatus> {
    const status = this.tasks.get(taskId);
    if (!status) {
      throw new Error(`Task not found: ${taskId}`);
    }
    return status;
  }

  /**
   * Stop task execution
   * @param taskId Task ID
   */
  async stopTask(taskId: string): Promise<void> {
    const status = this.tasks.get(taskId);
    if (!status) {
      throw new Error(`Task not found: ${taskId}`);
    }

    if (
      status.status === "completed" ||
      status.status === "failed" ||
      status.status === "cancelled"
    ) {
      throw new Error(`Task cannot be stopped: ${taskId}, current status: ${status.status}`);
    }

    // Update task status
    this.updateTaskStatus(taskId, {
      status: "cancelled",
      message: "The task has been cancelled",
    });

    output.infoLog(`Task stopped: ${taskId}`);
  }

  /**
   * Update task status
   * @param taskId Task ID
   * @param updates Status updates
   */
  updateTaskStatus(taskId: string, updates: Partial<TaskStatus>): void {
    const status = this.tasks.get(taskId);
    if (status) {
      Object.assign(status, updates, { lastUpdate: new Date() });
      output.debugLog(`Task status updated: ${taskId}, status: ${status.status}`);
    }
  }

  /**
   * Get all tasks
   * @returns List of all task statuses
   */
  getAllTasks(): TaskStatus[] {
    return Array.from(this.tasks.values());
  }

  /**
   * Get active tasks
   * @returns List of active tasks
   */
  getActiveTasks(): TaskStatus[] {
    return Array.from(this.tasks.values()).filter(task => task.status === "running");
  }

  /**
   * Get terminal ID associated with task
   * @param taskId Task ID
   * @returns Terminal session ID
   */
  getTerminalId(taskId: string): string | undefined {
    return this.taskTerminalMap.get(taskId);
  }

  /**
   * Cleanup completed or failed tasks
   * @param maxAge Maximum retention time in milliseconds
   */
  cleanupOldTasks(maxAge: number = 3600000): void {
    const now = Date.now();
    const tasksToRemove: string[] = [];

    this.tasks.forEach((status, taskId) => {
      const age = now - status.lastUpdate.getTime();
      if (
        age > maxAge &&
        (status.status === "completed" ||
          status.status === "failed" ||
          status.status === "cancelled")
      ) {
        tasksToRemove.push(taskId);
      }
    });

    tasksToRemove.forEach(taskId => {
      this.tasks.delete(taskId);
      this.taskTerminalMap.delete(taskId);
    });

    if (tasksToRemove.length > 0) {
      output.infoLog(`Cleaned up ${tasksToRemove.length} old task(s)`);
    }
  }

  /**
   * Build execution command
   * @param workflowId Workflow ID
   * @param input Input data
   * @param taskId Task ID
   * @returns Execution command string
   */
  private buildExecutionCommand(
    workflowId: string,
    input: Record<string, unknown>,
    taskId: string,
  ): string {
    // Escape special characters in the input data
    const inputJson = JSON.stringify(input).replace(/"/g, '\\"').replace(/'/g, "\\'");

    // Build command
    let command = `modular-agent thread run ${workflowId}`;

    if (Object.keys(input).length > 0) {
      command += ` --input '${inputJson}'`;
    }

    command += ` --task-id ${taskId}`;

    return command;
  }

  /**
   * Mark task as completed
   * @param taskId Task ID
   * @param message Completion message
   */
  markTaskCompleted(taskId: string, message?: string): void {
    this.updateTaskStatus(taskId, {
      status: "completed",
      progress: 100,
      message: message || "Task completed",
    });
  }

  /**
   * Mark task as failed
   * @param taskId Task ID
   * @param error Error message
   */
  markTaskFailed(taskId: string, error: string): void {
    this.updateTaskStatus(taskId, {
      status: "failed",
      message: `Task failed: ${error}`,
    });
  }

  /**
   * Update task progress
   * @param taskId Task ID
   * @param progress Progress percentage (0-100)
   * @param message Progress message
   */
  updateTaskProgress(taskId: string, progress: number, message?: string): void {
    this.updateTaskStatus(taskId, {
      progress: Math.min(100, Math.max(0, progress)),
      message: message || `Progress: ${progress}%`,
    });
  }
}
