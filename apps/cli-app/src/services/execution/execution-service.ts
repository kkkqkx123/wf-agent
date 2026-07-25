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
import type { WorkflowExecutionResult, BaseEvent, NodeCompletedEvent, WorkflowExecutionCompletedEvent, WorkflowExecutionFailedEvent, WorkflowExecutionMode } from "@wf-agent/types";
import { createExecutionScopedSubscription } from "@wf-agent/sdk/api";
import { CLIError, ErrorCode } from "../../types/cli-types.js";
import { getMode } from "../../utils/mode-detector.js";

const output = getOutput();

/**
 * Execution result interface
 * Uses shared WorkflowExecutionMode from @wf-agent/types.
 */
export interface ExecutionResult {
  /** Execution mode */
  mode: WorkflowExecutionMode;
  /** Execution ID */
  executionId: string;
  /** Workflow ID */
  workflowId: string;
  /** Execution status */
  status: string;
  /** Start time */
  startTime: Date;
  /** Terminal session ID (for foreground/background modes) */
  terminalId?: string;
  /** Process ID (for foreground/background modes) */
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
  private sdk: SDKInstance;

  constructor(sdk: SDKInstance, terminalManager: TerminalManager) {
    this.sdk = sdk;
    this.terminalManager = terminalManager;
    this.adapter = new WorkflowExecutionAdapter();
  }

  /**
   * Execute workflow with specified mode
   * @param workflowId Workflow ID to execute
   * @param input Input data for the workflow
   * @param mode Execution mode (blocking/foreground/background)
   * @returns Execution result
   */
  async execute(
    workflowId: string,
    input: Record<string, unknown>,
    mode: WorkflowExecutionMode = 'foreground'
  ): Promise<ExecutionResult> {
    output.infoLog(`Executing workflow ${workflowId} in ${mode} mode`);

    // Validate application runtime mode × workflow execution mode combination
    const validatedMode = this.validateModeCombination(mode);

    switch (validatedMode) {
      case 'blocking':
        return this.executeBlocking(workflowId, input);
      case 'foreground':
        return this.executeForeground(workflowId, input);
      case 'background':
        return this.executeBackground(workflowId, input);
      default:
        throw new CLIError(
          `Unsupported execution mode: ${mode}`,
          ErrorCode.VALIDATION
        );
    }
  }

  /**
   * Validate application runtime mode × workflow execution mode combination.
   *
   * Mode combination matrix:
   *   app mode ↓  \  exec mode → | foreground (default) | blocking | background
   *   ---------------------------|-----------------------|----------|------------
   *   interactive                | ✓ (default)           | ✓        | ✓
   *   headless                   | ⚠ → blocking         | ✓        | ⚠ → blocking
   *   programmatic               | ⚠ → blocking         | ✓        | ✓
   *
   * Legend:
   *   ✓ = valid, used as-is
   *   ⚠ → = invalid, auto-downgraded with warning
   *
   * @param requestedMode The workflow execution mode requested by the user/command
   * @returns The validated (possibly adjusted) mode
   */
  private validateModeCombination(requestedMode: WorkflowExecutionMode): WorkflowExecutionMode {
    const appMode = getMode();

    // Interactive mode: all execution modes are valid
    if (appMode.isInteractive) {
      return requestedMode;
    }

    // Headless mode: foreground and background create unnecessary terminals/logging.
    if (appMode.isHeadless) {
      if (requestedMode === 'foreground' || requestedMode === 'background') {
        output.warnLog(
          `Headless mode does not support '${requestedMode}' execution. ` +
          "Falling back to 'blocking' mode for clean output."
        );
        return 'blocking';
      }
      return requestedMode;
    }

    // Programmatic mode: foreground creates an unwanted terminal, fall back to blocking
    if (appMode.isProgrammatic) {
      if (requestedMode === 'foreground') {
        output.warnLog(
          "Programmatic mode does not support 'foreground' execution. " +
          "Falling back to 'blocking' mode for structured result output."
        );
        return 'blocking';
      }
      return requestedMode;
    }

    // Unknown app mode: pass through unchanged
    return requestedMode;
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
   * Foreground mode: SDK async execution + real-time event display via terminal
   *
   * - Single SDK instance (shared with CLI process)
   * - Asynchronous execution — CLI returns immediately
   * - A node-pty pseudo-terminal is created to display real-time execution events
   * - The workflow runs IN-PROCESS with the CLI, NOT in a separate OS process
   * - If the CLI exits, the in-process workflow execution is terminated
   * - User can view progress via the pseudo-terminal, but cannot interact with it
   *
   * NOTE: The workflow runs in the same process as the CLI. True OS-level
   * detachment is NOT provided by this mode. For background execution that
   * survives CLI exit, use --background mode.
   */
  private async executeForeground(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    output.debugLog('Starting foreground execution');

    // 1. Start workflow via SDK (single initialization)
    const execution = await this.adapter.executeWorkflow(workflowId, input);

    // Extract execution ID
    const executionId = 'executionId' in execution ? execution.executionId : (execution as any).id;

    // 2. Create foreground terminal for display
    const terminal = this.terminalManager.createTerminal({
      background: false,
    });

    output.infoLog(`Foreground execution started in terminal ${terminal.id}`);

    // 3. Display initial information in terminal
    this.displayExecutionInfo(terminal, {
      workflowId,
      executionId: executionId || '',
      mode: 'foreground',
    });

    // 4. Stream events to terminal (if SDK supports event subscription)
    this.setupEventStreaming(executionId || '', terminal).catch(error => {
      output.errorLog(`Failed to setup event streaming: ${error.message}`);
    });

    return {
      mode: 'foreground',
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
   * Subscribe to execution events and write output to the terminal
   * Shared implementation used by both event streaming and background logging.
   * Uses plain text format for all messages.
   */
  private subscribeToExecutionEvents(
    executionId: string,
    terminal: TerminalSession,
    options?: { includeNodeStarted?: boolean }
  ): void {
    const factory = this.sdk.getFactory();
    if (!factory) {
      output.warnLog('SDK factory not available, skipping execution event subscription');
      return;
    }
    const dependencies = factory.getDependencies();
    if (!dependencies) {
      output.warnLog('SDK dependencies not available, skipping execution event subscription');
      return;
    }

    // Helper to write to terminal (works with both IPty and child process stdin)
    const writeToTerminal = (message: string): void => {
      if (terminal.pty && 'write' in terminal.pty) {
        terminal.pty.write(message);
      } else if (terminal.pty && 'stdin' in terminal.pty && terminal.pty.stdin) {
        terminal.pty.stdin.write(message);
      }
    };

    let nodeStartedUnsubscribe: (() => void) | undefined;

    // Optionally subscribe to node started events
    if (options?.includeNodeStarted) {
      nodeStartedUnsubscribe = createExecutionScopedSubscription(
        executionId,
        'NODE_STARTED',
        (event: BaseEvent) => {
          const message = `[${new Date().toISOString()}] → Node started: ${(event as any).nodeId} (${(event as any).nodeType})\n`;
          writeToTerminal(message);
        },
        dependencies
      ).subscribe();
    }

    // Subscribe to node completed events
    const nodeCompletedUnsubscribe = createExecutionScopedSubscription(
      executionId,
      'NODE_COMPLETED',
      (event: BaseEvent) => {
        const nodeEvent = event as NodeCompletedEvent;
        const message = `[${new Date().toISOString()}] ✓ Node completed: ${nodeEvent.nodeId}\n`;
        writeToTerminal(message);
      },
      dependencies
    ).subscribe();

    // Subscribe to workflow completed event
    const workflowCompletedUnsubscribe = createExecutionScopedSubscription(
      executionId,
      'WORKFLOW_EXECUTION_COMPLETED',
      (event: BaseEvent) => {
        const workflowEvent = event as WorkflowExecutionCompletedEvent;
        const message = [
          `[${new Date().toISOString()}] Workflow execution completed`,
          `  Execution Time: ${(workflowEvent.executionTime / 1000).toFixed(2)}s`,
          '',
        ].join('\n');

        writeToTerminal(message);

        // Cleanup subscriptions
        if (nodeStartedUnsubscribe) nodeStartedUnsubscribe();
        nodeCompletedUnsubscribe();
        workflowCompletedUnsubscribe();
      },
      dependencies
    ).subscribe();

    // Subscribe to workflow failed event
    const workflowFailedUnsubscribe = createExecutionScopedSubscription(
      executionId,
      'WORKFLOW_EXECUTION_FAILED',
      (event: BaseEvent) => {
        const workflowEvent = event as WorkflowExecutionFailedEvent;
        const errorMessage = workflowEvent.error instanceof Error
          ? workflowEvent.error.message
          : String(workflowEvent.error);

        const message = [
          `[${new Date().toISOString()}] Workflow execution failed`,
          `  Error: ${errorMessage}`,
          '',
        ].join('\n');

        writeToTerminal(message);

        // Cleanup subscriptions
        if (nodeStartedUnsubscribe) nodeStartedUnsubscribe();
        nodeCompletedUnsubscribe();
        workflowFailedUnsubscribe();
      },
      dependencies
    ).subscribe();
  }

  /**
   * Follow an execution in real-time by streaming events to stdout.
   * Unlike foreground mode (which uses node-pty), this writes directly to
   * the main terminal so users can observe execution progress in headless
   * or remote scenarios.
   *
   * Returns a promise that resolves when the execution completes or fails.
   *
   * @param executionId Execution ID to follow
   */
  async followExecution(executionId: string): Promise<void> {
    output.infoLog(`Following execution: ${executionId}`);

    const factory = this.sdk.getFactory();
    if (!factory) {
      output.warnLog('SDK factory not available, cannot follow execution');
      return;
    }
    const dependencies = factory.getDependencies();
    if (!dependencies) {
      output.warnLog('SDK dependencies not available, cannot follow execution');
      return;
    }

    return new Promise<void>((resolve) => {
      // Subscribe to node started events
      const nodeStartedUnsubscribe = createExecutionScopedSubscription(
        executionId,
        'NODE_STARTED',
        (event: BaseEvent) => {
          const message = `[${new Date().toISOString()}] → Node started: ${(event as any).nodeId} (${(event as any).nodeType})`;
          output.infoLog(message);
        },
        dependencies
      ).subscribe();

      // Subscribe to node completed events
      const nodeCompletedUnsubscribe = createExecutionScopedSubscription(
        executionId,
        'NODE_COMPLETED',
        (event: BaseEvent) => {
          const nodeEvent = event as NodeCompletedEvent;
          const message = `[${new Date().toISOString()}] ✓ Node completed: ${nodeEvent.nodeId}`;
          output.infoLog(message);
        },
        dependencies
      ).subscribe();

      // Subscribe to workflow completed event
      createExecutionScopedSubscription(
        executionId,
        'WORKFLOW_EXECUTION_COMPLETED',
        (event: BaseEvent) => {
          const workflowEvent = event as WorkflowExecutionCompletedEvent;
          output.infoLog(`[${new Date().toISOString()}] Workflow execution completed (${(workflowEvent.executionTime / 1000).toFixed(2)}s)`);

          // Cleanup subscriptions
          nodeStartedUnsubscribe();
          nodeCompletedUnsubscribe();
          resolve();
        },
        dependencies
      ).subscribe();

      // Subscribe to workflow failed event
      createExecutionScopedSubscription(
        executionId,
        'WORKFLOW_EXECUTION_FAILED',
        (event: BaseEvent) => {
          const workflowEvent = event as WorkflowExecutionFailedEvent;
          const errorMessage = workflowEvent.error instanceof Error
            ? workflowEvent.error.message
            : String(workflowEvent.error);
          output.errorLog(`[${new Date().toISOString()}] Workflow execution failed: ${errorMessage}`);

          // Cleanup subscriptions
          nodeStartedUnsubscribe();
          nodeCompletedUnsubscribe();
          resolve();
        },
        dependencies
      ).subscribe();
    });
  }

  /**
   * Setup event streaming to terminal
   * Subscribes to SDK events and forwards them to the terminal for real-time display
   */
  private async setupEventStreaming(
    executionId: string,
    terminal: TerminalSession
  ): Promise<void> {
    this.subscribeToExecutionEvents(executionId, terminal);
    output.debugLog(`Event streaming setup for execution ${executionId}`);
  }

  /**
   * Setup background logging
   * Subscribes to SDK events and writes them to log file via background terminal
   */
  private async setupBackgroundLogging(
    executionId: string,
    terminal: TerminalSession
  ): Promise<void> {
    this.subscribeToExecutionEvents(executionId, terminal, { includeNodeStarted: true });
    output.debugLog(`Background logging setup for execution ${executionId}`);
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      output.errorLog(`Failed to monitor execution ${executionId}: ${errorMessage}`);
      throw new CLIError(
        `Failed to monitor execution: ${errorMessage}`,
        ErrorCode.API,
        4
      );
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      output.errorLog(`Failed to stop execution ${executionId}: ${errorMessage}`);
      throw new CLIError(
        `Failed to stop workflow execution: ${errorMessage}`,
        ErrorCode.API,
        4,
      );
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
