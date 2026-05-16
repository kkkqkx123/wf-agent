/**
 * Execution Service
 * Unified execution layer for workflow executions.
 * 
 * Design Principle: All workflow executions go through SDK.
 * Terminals are used ONLY for display/output purposes, not for execution.
 */

import { getOutput } from "../../utils/output.js";
import type { SDKInstance } from "@wf-agent/sdk/api";
import { WorkflowExecutionAdapter } from "../../adapters/workflow-execution-adapter.js";
import { TerminalManager } from "../terminal/terminal-manager.js";
import type { TerminalSession } from "../terminal/types.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";

const output = getOutput();

/**
 * Execution mode types
 */
export type ExecutionMode = 'blocking' | 'detached' | 'background';

/**
 * Execution result interface
 */
export interface ExecutionResult {
  /** Execution mode */
  mode: ExecutionMode;
  /** Execution ID */
  executionId: string;
  /** Workflow ID */
  workflowId: string;
  /** Status */
  status: string;
  /** Start time */
  startTime: Date;
  /** Terminal session ID (for detached/background modes) */
  terminalId?: string;
  /** Process ID (for detached/background modes) */
  pid?: number;
  /** Log file path (for background mode) */
  logFile?: string;
  /** Execution result data (for blocking mode) */
  result?: WorkflowExecutionResult;
}

/**
 * Execution Service
 * Provides unified interface for workflow execution across different modes.
 */
export class ExecutionService {
  private terminalManager: TerminalManager;
  private adapter: WorkflowExecutionAdapter;

  constructor(_sdk: SDKInstance, terminalManager: TerminalManager) {
    this.terminalManager = terminalManager;
    this.adapter = new WorkflowExecutionAdapter();
  }

  /**
   * Execute workflow with specified mode
   * @param workflowId Workflow ID to execute
   * @param input Input data for the workflow
   * @param mode Execution mode (blocking/detached/background)
   * @returns Execution result
   */
  async execute(
    workflowId: string,
    input: Record<string, unknown>,
    mode: ExecutionMode = 'detached'
  ): Promise<ExecutionResult> {
    output.infoLog(`Executing workflow ${workflowId} in ${mode} mode`);

    switch (mode) {
      case 'blocking':
        return this.executeBlocking(workflowId, input);
      case 'detached':
        return this.executeDetached(workflowId, input);
      case 'background':
        return this.executeBackground(workflowId, input);
      default:
        throw new Error(`Unsupported execution mode: ${mode}`);
    }
  }

  /**
   * Blocking mode: Direct SDK call, wait for completion
   * - Single SDK instance (shared with CLI)
   * - Synchronous execution
   * - Returns final result
   */
  private async executeBlocking(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    output.debugLog('Starting blocking execution');

    // Execute via SDK adapter
    const execution = await this.adapter.executeWorkflow(workflowId, input);

    // Extract execution ID - handle both WorkflowExecution and WorkflowExecutionResult types
    const executionId = 'executionId' in execution ? execution.executionId : (execution as any).id;
    
    output.infoLog(`Blocking execution completed: ${executionId || 'unknown'}`);

    return {
      mode: 'blocking',
      executionId: executionId || '',
      workflowId,
      status: execution.status || 'completed',
      startTime: new Date(execution.createdAt || Date.now()),
      result: execution as WorkflowExecutionResult,
    };
  }

  /**
   * Detached mode: SDK execution + terminal for real-time display
   * - Single SDK instance (shared with CLI)
   * - Asynchronous execution
   * - Terminal shows real-time progress
   * - User can interact with terminal
   */
  private async executeDetached(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    output.debugLog('Starting detached execution');

    // 1. Start workflow via SDK (single initialization)
    const execution = await this.adapter.executeWorkflow(workflowId, input);

    // Extract execution ID
    const executionId = 'executionId' in execution ? execution.executionId : (execution as any).id;

    // 2. Create foreground terminal for display
    const terminal = this.terminalManager.createTerminal({
      background: false,
    });

    output.infoLog(`Detached execution started in terminal ${terminal.id}`);

    // 3. Display initial information in terminal
    this.displayExecutionInfo(terminal, {
      workflowId,
      executionId: executionId || '',
      mode: 'detached',
    });

    // 4. Stream events to terminal (if SDK supports event subscription)
    // Note: This is a placeholder for future event streaming implementation
    // For now, the terminal will show the CLI command output if user runs commands manually
    this.setupEventStreaming(executionId || '', terminal).catch(error => {
      output.errorLog(`Failed to setup event streaming: ${error.message}`);
    });

    return {
      mode: 'detached',
      executionId: executionId || '',
      workflowId,
      status: execution.status || 'running',
      startTime: new Date(execution.createdAt || Date.now()),
      terminalId: terminal.id,
      pid: terminal.pid,
    };
  }

  /**
   * Background mode: SDK execution + log file
   * - Single SDK instance (shared with CLI)
   * - Asynchronous execution
   * - Output redirected to log file
   * - No interactive terminal
   */
  private async executeBackground(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    output.debugLog('Starting background execution');

    // 1. Start workflow via SDK (single initialization)
    const execution = await this.adapter.executeWorkflow(workflowId, input);

    // Extract execution ID
    const executionId = 'executionId' in execution ? execution.executionId : (execution as any).id;

    // 2. Create background terminal for logging
    const logFile = `logs/workflow-${executionId || 'unknown'}.log`;
    const terminal = this.terminalManager.createTerminal({
      background: true,
      logFile,
    });

    output.infoLog(`Background execution started, log file: ${logFile}`);

    // 3. Write initial information to log
    this.logExecutionInfo(terminal, {
      workflowId,
      executionId: executionId || '',
      mode: 'background',
      startTime: new Date(),
    });

    // 4. Setup background logging
    this.setupBackgroundLogging(executionId || '', terminal).catch(error => {
      output.errorLog(`Failed to setup background logging: ${error.message}`);
    });

    return {
      mode: 'background',
      executionId: executionId || '',
      workflowId,
      status: execution.status || 'running',
      startTime: new Date(execution.createdAt || Date.now()),
      terminalId: terminal.id,
      pid: terminal.pid,
      logFile,
    };
  }

  /**
   * Display execution information in terminal
   */
  private displayExecutionInfo(
    terminal: TerminalSession,
    info: {
      workflowId: string;
      executionId: string;
      mode: string;
    }
  ): void {
    const message = [
      '╔══════════════════════════════════════════╗',
      '║     Workflow Execution Started           ║',
      '╠══════════════════════════════════════════╣',
      `║ Workflow: ${info.workflowId.padEnd(30)}║`,
      `║ Execution: ${info.executionId.padEnd(28)}║`,
      `║ Mode: ${info.mode.padEnd(33)}║`,
      '╚══════════════════════════════════════════╝',
      '',
      'This terminal displays real-time execution progress.',
      'The workflow is running via SDK in the background.',
      '',
    ].join('\n');

    // Only foreground terminals support write
    if (terminal.pty && 'write' in terminal.pty) {
      terminal.pty.write(message);
    }
  }

  /**
   * Log execution information to background terminal
   */
  private logExecutionInfo(
    terminal: TerminalSession,
    info: {
      workflowId: string;
      executionId: string;
      mode: string;
      startTime: Date;
    }
  ): void {
    const message = [
      `[${info.startTime.toISOString()}] Workflow Execution Started`,
      `  Workflow ID: ${info.workflowId}`,
      `  Execution ID: ${info.executionId}`,
      `  Mode: ${info.mode}`,
      `  Start Time: ${info.startTime.toISOString()}`,
      '',
    ].join('\n');

    // Background terminals use ChildProcess with stdin
    if (terminal.pty && 'stdin' in terminal.pty && terminal.pty.stdin) {
      terminal.pty.stdin.write(message);
    }
  }

  /**
   * Setup event streaming to terminal (placeholder for future implementation)
   * This will subscribe to SDK events and forward them to the terminal
   */
  private async setupEventStreaming(
    _executionId: string,
    _terminal: TerminalSession
  ): Promise<void> {
    // TODO: Implement event streaming when SDK provides event subscription API
    // Example:
    // this.sdk.events.on('workflow.progress', (event) => {
    //   if (event.executionId === executionId) {
    //     terminal.pty.write(`Progress: ${event.progress}%\n`);
    //   }
    // });
    
    output.debugLog('Event streaming setup (placeholder - not yet implemented)');
  }

  /**
   * Setup background logging (placeholder for future implementation)
   * This will subscribe to SDK events and write them to log file
   */
  private async setupBackgroundLogging(
    _executionId: string,
    _terminal: TerminalSession
  ): Promise<void> {
    // TODO: Implement background logging when SDK provides event subscription API
    // Example:
    // this.sdk.events.on('workflow.progress', (event) => {
    //   if (event.executionId === executionId) {
    //     terminal.pty.stdin?.write(`[${new Date().toISOString()}] Progress: ${event.progress}%\n`);
    //   }
    // });
    
    output.debugLog('Background logging setup (placeholder - not yet implemented)');
  }

  /**
   * Monitor execution status
   * @param executionId Execution ID
   * @returns Current execution status
   */
  async monitorExecution(executionId: string): Promise<{
    executionId: string;
    status: string;
    progress?: number;
    lastUpdate: Date;
  }> {
    try {
      const execution = await this.adapter.getWorkflowExecution(executionId);
      
      return {
        executionId,
        status: execution.status || 'unknown',
        lastUpdate: new Date(),
      };
    } catch (error) {
      output.errorLog(`Failed to monitor execution ${executionId}: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }
  }

  /**
   * Stop execution
   * @param executionId Execution ID
   */
  async stopExecution(executionId: string): Promise<void> {
    output.infoLog(`Stopping execution: ${executionId}`);
    
    try {
      await this.adapter.stopWorkflowExecution(executionId);
      output.infoLog(`Execution stopped: ${executionId}`);
    } catch (error) {
      output.errorLog(`Failed to stop execution ${executionId}: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    output.infoLog('Cleaning up ExecutionService resources...');
    
    // Cleanup all terminals
    await this.terminalManager.cleanupAll();
    
    output.infoLog('ExecutionService cleanup completed');
  }
}
