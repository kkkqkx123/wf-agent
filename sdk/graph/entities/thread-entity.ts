/**
 * ThreadEntity - A pure data entity that encapsulates the data access operations for Thread instances.
 * Refer to the design pattern of AgentLoopEntity.
 *
 * Notes:
 * - The factory method is provided by ThreadBuilder.
 * - Lifecycle management is handled by ThreadLifecycleCoordinator and ThreadLifecycleCoordinator.
 * - The snapshot functionality is provided by ThreadSnapshotManager.
 * - ConversationSession management is handled by ThreadStateCoordinator (not held directly).
 *
 */

import type { ID, LLMMessage, NodeExecutionResult } from "@wf-agent/types";
import type { Thread, WorkflowExecutionStatus, WorkflowExecutionType } from "@wf-agent/types";
import type { PreprocessedGraph } from "@wf-agent/types";
import type { SubgraphContext } from "../state-managers/execution-state.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { ThreadState } from "../state-managers/thread-state.js";
import { MessageHistory } from "../../agent/message/message-history.js";
import { VariableState } from "../state-managers/variable-state.js";

/**
 * ThreadEntity - Thread Entity
 *
 * Core Responsibilities:
 * - Encapsulates all data of the execution instance
 * - Provides data access interfaces (getter/setter)
 * - Holds an instance of the state manager
 *
 * Design Principles:
 * - Pure data entity: Contains only data and access methods
 * - Does not include factory methods: This is the responsibility of ThreadBuilder
 * - Does not include lifecycle methods: This is the responsibility of ThreadLifecycleCoordinator/Coordinator
 * - ConversationSession is managed by ThreadStateCoordinator, not held directly
 *
 * Migration Note:
 * - conversationManager has been removed from ThreadEntity
 * - Use ThreadStateCoordinator for unified state management
 * - This eliminates data redundancy and synchronization issues
 */
export class ThreadEntity {
  /** Execute instance ID */
  readonly id: string;

  /** Thread data object (private, not exposed) */
  private readonly thread: Thread;

  /** Execution Status */
  readonly state: ThreadState;

  /** Execution Status Manager (Subgraph Execution Stack) */
  private readonly executionState: ExecutionState;

  /** Message History Manager */
  readonly messageHistoryManager: MessageHistory;

  /** Variable State Manager */
  readonly variableStateManager: VariableState;

  /** Stop Controller */
  abortController?: AbortController;

  /** Trigger Management */
  triggerManager?: unknown;

  /** Tool Visibility Coordinator */
  toolVisibilityCoordinator?: unknown;

  /** Child AgentLoop IDs (for lifecycle management) */
  private childAgentLoopIds: Set<string> = new Set();

  /**
   * Constructor
   * @param thread: Thread data object
   * @param executionState: Execution state manager
   * @param state: Execution status (optional; a new instance is created by default)
   */
  constructor(thread: Thread, executionState: ExecutionState, state?: ThreadState) {
    this.id = thread.id;
    this.thread = thread;
    this.executionState = executionState;
    this.state = state ?? new ThreadState();
    this.messageHistoryManager = new MessageHistory(thread.id);
    this.variableStateManager = new VariableState(thread.id);
  }

  // Basic Property Access ============

  getWorkflowId(): string {
    return this.thread.workflowId;
  }

  getStatus(): WorkflowExecutionStatus {
    return this.state.status;
  }

  setStatus(status: WorkflowExecutionStatus): void {
    this.state.status = status;
  }

  getThreadType(): WorkflowExecutionType {
    return this.thread.threadType || "MAIN";
  }

  setThreadType(threadType: WorkflowExecutionType): void {
    this.thread.threadType = threadType;
  }

  getCurrentNodeId(): string {
    return this.thread.currentNodeId;
  }

  setCurrentNodeId(nodeId: string): void {
    this.thread.currentNodeId = nodeId;
  }

  getInput(): Record<string, unknown> {
    return this.thread.input;
  }

  getOutput(): Record<string, unknown> {
    return this.thread.output;
  }

  setOutput(output: Record<string, unknown>): void {
    this.thread.output = output;
  }

  // Execution Results ============

  addNodeResult(result: NodeExecutionResult): void {
    this.thread.nodeResults.push(result);
  }

  getNodeResults(): NodeExecutionResult[] {
    return this.thread.nodeResults as NodeExecutionResult[];
  }

  // Error Message

  getErrors(): unknown[] {
    return this.thread.errors;
  }

  // ========== Time Information ----------

  getStartTime(): number | null {
    return this.state.startTime;
  }

  getEndTime(): number | null {
    return this.state.endTime;
  }

  // ========== Image Navigation ----------

  getGraph(): PreprocessedGraph {
    return this.thread.graph;
  }

  // ========== Subgraph Execution Status ----------

  enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: unknown): void {
    this.executionState.enterSubgraph(workflowId, parentWorkflowId, input);
  }

  exitSubgraph(): void {
    this.executionState.exitSubgraph();
  }

  getCurrentSubgraphContext(): SubgraphContext | null {
    return this.executionState.getCurrentSubgraphContext();
  }

  getSubgraphStack(): SubgraphContext[] {
    return this.executionState.getSubgraphStack();
  }

  // ========== Fork/Join Context ----------

  setForkId(forkId: string): void {
    if (!this.thread.forkJoinContext) {
      this.thread.forkJoinContext = { forkId, forkPathId: "" };
    }
    this.thread.forkJoinContext.forkId = forkId;
  }

  setForkPathId(forkPathId: string): void {
    if (!this.thread.forkJoinContext) {
      this.thread.forkJoinContext = { forkId: "", forkPathId };
    }
    this.thread.forkJoinContext.forkPathId = forkPathId;
  }

  // Triggering Sub-workflow Context ============

  registerChildThread(childThreadId: ID): void {
    if (!this.thread.triggeredSubworkflowContext) {
      this.thread.triggeredSubworkflowContext = {
        parentThreadId: "",
        childThreadIds: [],
        triggeredSubworkflowId: "",
      };
    }
    if (!this.thread.triggeredSubworkflowContext.childThreadIds) {
      this.thread.triggeredSubworkflowContext.childThreadIds = [];
    }
    if (!this.thread.triggeredSubworkflowContext.childThreadIds.includes(childThreadId)) {
      this.thread.triggeredSubworkflowContext.childThreadIds.push(childThreadId);
    }
  }

  unregisterChildThread(childThreadId: ID): void {
    if (this.thread.triggeredSubworkflowContext?.childThreadIds) {
      this.thread.triggeredSubworkflowContext.childThreadIds =
        this.thread.triggeredSubworkflowContext.childThreadIds.filter(
          (id: ID) => id !== childThreadId,
        );
    }
  }

  getParentThreadId(): ID | undefined {
    return this.thread.triggeredSubworkflowContext?.parentThreadId;
  }

  getChildThreadIds(): ID[] {
    return this.thread.triggeredSubworkflowContext?.childThreadIds || [];
  }

  setParentThreadId(parentThreadId: ID): void {
    if (!this.thread.triggeredSubworkflowContext) {
      this.thread.triggeredSubworkflowContext = {
        parentThreadId,
        childThreadIds: [],
        triggeredSubworkflowId: "",
      };
    }
    this.thread.triggeredSubworkflowContext.parentThreadId = parentThreadId;
  }

  getTriggeredSubworkflowId(): ID | undefined {
    return this.thread.triggeredSubworkflowContext?.triggeredSubworkflowId;
  }

  setTriggeredSubworkflowId(subworkflowId: ID): void {
    if (!this.thread.triggeredSubworkflowContext) {
      this.thread.triggeredSubworkflowContext = {
        parentThreadId: "",
        childThreadIds: [],
        triggeredSubworkflowId: subworkflowId,
      };
    }
    this.thread.triggeredSubworkflowContext.triggeredSubworkflowId = subworkflowId;
  }

  // Message Management ============

  /**
   * Add a message
   * @param message LLM message
   * @deprecated Use ThreadStateCoordinator.addMessage() instead for unified state management
   */
  addMessage(message: LLMMessage): void {
    this.messageHistoryManager.addMessage(message);
  }

  /**
   * Get all messages
   */
  getMessages(): LLMMessage[] {
    return this.messageHistoryManager.getMessages();
  }

  /**
   * Get the latest messages
   * @param count Number of messages
   */
  getRecentMessages(count: number): LLMMessage[] {
    return this.messageHistoryManager.getRecentMessages(count);
  }

  /**
   * Set message history
   * @param messages List of messages
   */
  setMessages(messages: LLMMessage[]): void {
    this.messageHistoryManager.setMessages(messages);
  }

  /**
   * Clear the message history
   */
  clearMessages(): void {
    this.messageHistoryManager.clearMessages();
  }

  /**
   * Standardizing the message history
   */
  normalizeHistory(): void {
    this.messageHistoryManager.normalizeHistory();
  }

  // Variable Management ============

  /**
   * Get the variable
   * @param name Variable name
   */
  getVariable(name: string): unknown {
    return this.variableStateManager.getVariable(name);
  }

  /**
   * Set a variable
   * @param name: Variable name
   * @param value: Variable value
   */
  setVariable(name: string, value: unknown): void {
    this.variableStateManager.setVariable(name, value);
  }

  /**
   * Get all variables
   */
  getAllVariables(): Record<string, unknown> {
    return this.variableStateManager.getAllVariables();
  }

  /**
   * Delete the variable
   * @param name Variable name
   */
  deleteVariable(name: string): boolean {
    return this.variableStateManager.deleteVariable(name);
  }

  // Stop control ============

  /**
   * Get the abort signal
   */
  getAbortSignal(): AbortSignal {
    if (!this.abortController) {
      this.abortController = new AbortController();
    }
    return this.abortController.signal;
  }

  /**
   * Check if it has been aborted.
   */
  isAborted(): boolean {
    return this.abortController?.signal.aborted ?? false;
  }

  /**
   * Stop execution
   * @param reason (optional) Reason for stopping execution
   */
  abort(reason?: string): void {
    if (this.abortController) {
      this.abortController.abort(reason);
    }
  }

  // ========== Interrupt Control ==========

  /**
   * Pause execution
   */
  pause(): void {
    this.state.pause();
  }

  /**
   * Resume execution
   */
  resume(): void {
    this.state.resume();
  }

  /**
   * Stop execution
   */
  stop(): void {
    this.state.cancel();
    this.abort();
  }

  /**
   * Check whether it should be paused.
   */
  shouldPause(): boolean {
    return this.state.shouldPause();
  }

  /**
   * Check whether it should be stopped.
   */
  shouldStop(): boolean {
    return this.state.shouldStop();
  }

  /**
   * Interrupt execution
   * @param type Type of the interrupt
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this.state.interrupt(type);
    if (type === "STOP") {
      this.abort();
    }
  }

  /**
   * Reset the interrupt flag.
   */
  resetInterrupt(): void {
    this.state.resetInterrupt();
  }

  // Trigger Status ============

  getTriggerStateSnapshot(): { triggers: unknown[] } {
    return {
      triggers: (this.triggerManager as { getAll?: () => unknown[] })?.getAll?.() || [],
    };
  }

  restoreTriggerState(triggerStates: { triggers?: unknown[] }): void {
    if (this.triggerManager && triggerStates?.triggers) {
      for (const trigger of triggerStates.triggers) {
        (this.triggerManager as { restore?: (trigger: unknown) => void })?.restore?.(trigger);
      }
    }
  }

  // Event Construction (Automatic Context Filling)

  /**
   * Build an event (automatically fills in threadId, workflowId, nodeId)
   * @param builder Event builder function
   * @param params Event parameters (excluding context fields)
   * @returns Complete event object
   *
   * @example
   * const event = threadEntity.buildEvent(buildNodeStartedEvent, { nodeType: 'LLM' });
   */
  buildEvent<
    T extends {
      threadId?: string;
      workflowId?: string;
      nodeId?: string;
      timestamp: number;
      type: string;
    },
  >(
    builder: (params: T) => T,
    params: Omit<T, "threadId" | "workflowId" | "nodeId" | "timestamp" | "type">,
  ): T {
    return builder({
      ...params,
      threadId: this.id,
      workflowId: this.getWorkflowId(),
      nodeId: this.getCurrentNodeId(),
    } as T);
  }

  // ========== Internal Access (for framework internal use only) ----------

  /**
   * Get a reference to the underlying Thread object
   * @internal For framework internal use only. External code should use Entity accessor methods.
   */
  getThread(): Thread {
    return this.thread;
  }

  // ========== Resource Cleanup ==========

  /**
   * Cleanup all resources held by this entity
   * This method should be called when the entity is no longer needed.
   */
  cleanup(): void {
    // Cleanup message history
    this.messageHistoryManager.cleanup();

    // Cleanup variable state
    this.variableStateManager.cleanup();

    // Clear abort controller
    this.abortController = undefined;

    // Clear child AgentLoop IDs
    this.childAgentLoopIds.clear();
  }

  // ========== Child AgentLoop Management ==========

  /**
   * Register a child AgentLoop ID
   * @param agentLoopId AgentLoop instance ID
   */
  registerChildAgentLoop(agentLoopId: string): void {
    this.childAgentLoopIds.add(agentLoopId);
  }

  /**
   * Unregister a child AgentLoop ID
   * @param agentLoopId AgentLoop instance ID
   */
  unregisterChildAgentLoop(agentLoopId: string): void {
    this.childAgentLoopIds.delete(agentLoopId);
  }

  /**
   * Get all child AgentLoop IDs
   * @returns Array of AgentLoop IDs
   */
  getChildAgentLoopIds(): string[] {
    return Array.from(this.childAgentLoopIds);
  }

  /**
   * Check if a AgentLoop is a child of this thread
   * @param agentLoopId AgentLoop instance ID
   * @returns Whether the AgentLoop is a child
   */
  hasChildAgentLoop(agentLoopId: string): boolean {
    return this.childAgentLoopIds.has(agentLoopId);
  }

  /**
   * Get the count of child AgentLoops
   * @returns Number of child AgentLoops
   */
  getChildAgentLoopCount(): number {
    return this.childAgentLoopIds.size;
  }
}
