/**
 * WorkflowExecutionContextAPI - Workflow Execution Context Query API
 *
 * Provides comprehensive variable and context state queries for workflow execution.
 * Enables tracking of variable evolution and execution context throughout the workflow.
 *
 * Features:
 * - Variable snapshot at specific timestamps
 * - Complete execution context state
 * - Node-level input context
 * - Variable history tracking
 * - Context state transitions
 *
 * Phase 2 Implementation: Add variable and context state query capabilities to Workflow
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "WorkflowExecutionContextAPI" });

// ============================================================================
// Type Definitions: Variable Context
// ============================================================================

/**
 * Variable definition with scope information
 */
export interface VariableDefinitionWithScope {
  /** Variable name */
  name: string;
  /** Variable type */
  type: string;
  /** Variable scope */
  scope: "global" | "node_local" | "iteration_local" | "parameter";
  /** Default value */
  defaultValue?: unknown;
  /** Whether variable is mutable */
  isMutable: boolean;
  /** Variable description */
  description?: string;
}

/**
 * Variable value at a specific point in time
 */
export interface VariableValueSnapshot {
  /** Variable name */
  name: string;
  /** Current value */
  value: unknown;
  /** Variable type */
  type: string;
  /** When this value was set */
  timestamp: number;
  /** Source of this value (e.g., "input", "node_123", "branch_decision") */
  source?: string;
  /** Sequence number (for ordered tracking) */
  sequenceNo?: number;
}

/**
 * Variable snapshot at a specific time
 */
export interface VariableSnapshot {
  /** Execution ID */
  executionId: ID;
  /** Timestamp of snapshot */
  timestamp: number;
  /** Point-in-time description (e.g., "before node_123", "after branch") */
  description?: string;
  /** Variables at this point in execution */
  variables: VariableValueSnapshot[];
  /** Current node ID at this time */
  currentNodeId?: string;
}

/**
 * Variable history entry
 */
export interface VariableHistoryEntry {
  /** Variable name */
  name: string;
  /** Previous value */
  previousValue?: unknown;
  /** New value */
  newValue: unknown;
  /** Timestamp of change */
  timestamp: number;
  /** Node where change occurred */
  changedByNode?: string;
  /** Change reason */
  reason?: string;
  /** Sequence number */
  sequenceNo: number;
}

/**
 * Variable history for a specific variable
 */
export interface VariableHistory {
  /** Execution ID */
  executionId: ID;
  /** Variable name */
  variableName: string;
  /** Complete history entries */
  history: VariableHistoryEntry[];
  /** Initial value */
  initialValue?: unknown;
  /** Final value */
  finalValue?: unknown;
  /** Total changes */
  totalChanges: number;
}

// ============================================================================
// Type Definitions: Execution Context
// ============================================================================

/**
 * Node input context
 */
export interface NodeInputContext {
  /** Node ID */
  nodeId: string;
  /** Node name */
  nodeName: string;
  /** Node type */
  nodeType: string;
  /** Input parameters */
  inputParameters: Record<string, unknown>;
  /** Timestamp when node execution started */
  timestamp: number;
  /** Available context variables */
  availableVariables: VariableValueSnapshot[];
}

/**
 * Execution context snapshot
 */
export interface ExecutionContextSnapshot {
  /** Execution ID */
  executionId: ID;
  /** Timestamp of snapshot */
  timestamp: number;
  /** Current node being executed */
  currentNodeId?: string;
  /** Current node name */
  currentNodeName?: string;
  /** Global variables */
  globalVariables: Record<string, unknown>;
  /** All defined variables with their definitions */
  variableDefinitions: VariableDefinitionWithScope[];
  /** Variables currently in scope */
  scopedVariables: Record<string, Record<string, unknown>>;
  /** Execution progress (percentage 0-100) */
  executionProgress: number;
  /** Nodes completed so far */
  completedNodes: string[];
  /** Nodes pending execution */
  pendingNodes: string[];
  /** Failed/skipped nodes */
  skippedNodes: string[];
}

/**
 * Context state transition
 */
export interface ContextStateTransition {
  /** Transition ID */
  transitionId: string;
  /** From node */
  fromNode?: string;
  /** To node */
  toNode?: string;
  /** Type of transition */
  transitionType:
    | "sequential"
    | "conditional_branch"
    | "loop"
    | "parallel_fork"
    | "join"
    | "completion";
  /** Condition that triggered transition */
  condition?: string;
  /** Timestamp of transition */
  timestamp: number;
  /** Variables changed during transition */
  variablesChanged?: VariableHistoryEntry[];
}

/**
 * Context evolution record
 */
export interface ContextEvolution {
  /** Execution ID */
  executionId: ID;
  /** Start timestamp */
  startTime: number;
  /** End timestamp */
  endTime?: number;
  /** All context state transitions */
  transitions: ContextStateTransition[];
  /** Snapshots at key points */
  keySnapshots: ExecutionContextSnapshot[];
  /** Total variable changes */
  totalVariableChanges: number;
}

// ============================================================================
// Query & Filter Types
// ============================================================================

/**
 * Variable snapshot filter
 */
export interface VariableSnapshotFilter {
  /** Execution ID list */
  executionIds?: ID[];
  /** Specific execution ID */
  executionId?: ID;
  /** Variable name filter */
  variableName?: string;
  /** Time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
  /** Node ID filter */
  nodeId?: string;
}

/**
 * Context evolution filter
 */
export interface ContextEvolutionFilter {
  /** Execution ID */
  executionId?: ID;
  /** Start time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
  /** Transition type filter */
  transitionType?: string;
  /** Node filter */
  nodeId?: string;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * WorkflowExecutionContextAPI - Workflow Execution Context Query API
 *
 * Provides queries for:
 * - Variable snapshots at specific times
 * - Complete execution context state
 * - Node-level input context
 * - Variable history and evolution
 * - Context transitions
 */
export class WorkflowExecutionContextAPI extends QueryableResourceAPI<
  ExecutionContextSnapshot,
  string,
  ContextEvolutionFilter
> {
  private contextSnapshots: Map<string, ExecutionContextSnapshot> = new Map();
  private variableSnapshots: Map<string, VariableSnapshot[]> = new Map();
  private contextEvolutions: Map<string, ContextEvolution> = new Map();
  private variableHistories: Map<string, VariableHistory> = new Map();

  /**
   * Constructor
   * @param deps APIDependencyManager instance
   */
  constructor(deps: APIDependencyManager) {
    super();
    void deps; // Acknowledge parameter
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get execution context by ID
   */
  protected override async getResource(id: string): Promise<ExecutionContextSnapshot | null> {
    return this.contextSnapshots.get(id) ?? null;
  }

  /**
   * Get all execution contexts
   */
  protected override async getAllResources(): Promise<ExecutionContextSnapshot[]> {
    return Array.from(this.contextSnapshots.values());
  }

  /**
   * Apply filters to context snapshots
   */
  protected override applyFilter(
    records: ExecutionContextSnapshot[],
    filter: ContextEvolutionFilter,
  ): ExecutionContextSnapshot[] {
    let filtered = records;

    if (filter.executionId) {
      filtered = filtered.filter(r => r.executionId === filter.executionId);
    }

    if (filter.timeRange?.start !== undefined) {
      filtered = filtered.filter(r => r.timestamp >= filter.timeRange!.start!);
    }

    if (filter.timeRange?.end !== undefined) {
      filtered = filtered.filter(r => r.timestamp <= filter.timeRange!.end!);
    }

    if (filter.nodeId) {
      filtered = filtered.filter(r => r.currentNodeId === filter.nodeId);
    }

    return filtered;
  }

  // ============================================================================
  // Variable Snapshot Queries
  // ============================================================================

  /**
   * Get variable snapshot at a specific timestamp
   *
   * @param executionId Workflow execution ID
   * @param timestamp Point in time to get snapshot
   * @returns Variable snapshot at the specified time
   */
  async getVariableSnapshot(
    executionId: ID,
    timestamp: number,
  ): Promise<VariableSnapshot | null> {
    const snapshots = this.variableSnapshots.get(executionId as string);
    if (!snapshots) {
      logger.debug(`No variable snapshots found for execution ${executionId}`);
      return null;
    }

    // Find the closest snapshot to the requested timestamp
    let closest: VariableSnapshot | null = null;
    let minDiff = Infinity;

    for (const snapshot of snapshots) {
      const diff = Math.abs(snapshot.timestamp - timestamp);
      if (diff < minDiff) {
        minDiff = diff;
        closest = snapshot;
      }
    }

    return closest;
  }

  /**
   * Get all variable snapshots for an execution
   *
   * @param executionId Workflow execution ID
   * @returns All variable snapshots
   */
  async getVariableSnapshots(executionId: ID): Promise<VariableSnapshot[]> {
    return this.variableSnapshots.get(executionId as string) ?? [];
  }

  /**
   * Get variable snapshots within time range
   *
   * @param executionId Workflow execution ID
   * @param timeRange Time range for snapshots
   * @returns Filtered variable snapshots
   */
  async getVariableSnapshotsByTimeRange(
    executionId: ID,
    timeRange: { start: number; end: number },
  ): Promise<VariableSnapshot[]> {
    const snapshots = this.variableSnapshots.get(executionId as string) ?? [];
    return snapshots.filter(
      s => s.timestamp >= timeRange.start && s.timestamp <= timeRange.end,
    );
  }

  // ============================================================================
  // Execution Context Queries
  // ============================================================================

  /**
   * Get execution context at a specific point
   *
   * @param executionId Workflow execution ID
   * @returns Current execution context snapshot
   */
  async getExecutionContext(executionId: ID): Promise<ExecutionContextSnapshot | null> {
    return this.contextSnapshots.get(executionId as string) ?? null;
  }

  /**
   * Get node input context
   *
   * @param executionId Workflow execution ID
   * @param nodeId Node ID to get input context for
   * @returns Node input context with available variables
   */
  async getNodeInputContext(
    executionId: ID,
    nodeId: string,
  ): Promise<NodeInputContext | null> {
    const context = this.contextSnapshots.get(executionId as string);
    if (!context) {
      logger.debug(`Context not found for execution ${executionId}`);
      return null;
    }

    // Find the execution context for this node
    const evolutions = this.contextEvolutions.get(executionId as string);
    if (!evolutions) {
      logger.debug(`Context evolution not found for execution ${executionId}`);
      return null;
    }

    // Find the transition to this node
    const transition = evolutions.transitions.find(t => t.toNode === nodeId);
    if (!transition) {
      logger.debug(`No transition found to node ${nodeId}`);
      return null;
    }

    // Get variables available at this point
    const snapshot = evolutions.keySnapshots.find(
      s => Math.abs(s.timestamp - transition.timestamp) < 1000, // Within 1 second
    );

    return {
      nodeId,
      nodeName: context.currentNodeName ?? "Unknown",
      nodeType: "Unknown",
      inputParameters: {},
      timestamp: transition.timestamp,
      availableVariables: snapshot
        ? Object.entries(snapshot.globalVariables).map(([name, value]) => ({
            name,
            value,
            type: typeof value,
            timestamp: transition.timestamp,
          }))
        : [],
    };
  }

  // ============================================================================
  // Variable History Queries
  // ============================================================================

  /**
   * Get variable history
   *
   * @param executionId Workflow execution ID
   * @param variableName Name of variable to track
   * @returns Variable history
   */
  async getVariableHistory(
    executionId: ID,
    variableName: string,
  ): Promise<VariableHistory | null> {
    const key = `${executionId}:${variableName}`;
    return this.variableHistories.get(key) ?? null;
  }

  /**
   * Get all variable histories for an execution
   *
   * @param executionId Workflow execution ID
   * @returns All variable histories
   */
  async getAllVariableHistories(executionId: ID): Promise<VariableHistory[]> {
    const histories: VariableHistory[] = [];
    for (const [key, history] of this.variableHistories.entries()) {
      if (key.startsWith(`${executionId}:`)) {
        histories.push(history);
      }
    }
    return histories;
  }

  /**
   * Get variable changes in a time range
   *
   * @param executionId Workflow execution ID
   * @param timeRange Time range
   * @returns Variable changes in the range
   */
  async getVariableChangesByTimeRange(
    executionId: ID,
    timeRange: { start: number; end: number },
  ): Promise<VariableHistoryEntry[]> {
    const histories = await this.getAllVariableHistories(executionId);
    const changes: VariableHistoryEntry[] = [];

    for (const history of histories) {
      for (const entry of history.history) {
        if (entry.timestamp >= timeRange.start && entry.timestamp <= timeRange.end) {
          changes.push(entry);
        }
      }
    }

    return changes.sort((a, b) => a.timestamp - b.timestamp);
  }

  // ============================================================================
  // Context Evolution Queries
  // ============================================================================

  /**
   * Get context evolution
   *
   * @param executionId Workflow execution ID
   * @returns Context evolution record
   */
  async getContextEvolution(executionId: ID): Promise<ContextEvolution | null> {
    return this.contextEvolutions.get(executionId as string) ?? null;
  }

  /**
   * Get context state transitions
   *
   * @param executionId Workflow execution ID
   * @returns All state transitions
   */
  async getContextTransitions(executionId: ID): Promise<ContextStateTransition[]> {
    const evolution = this.contextEvolutions.get(executionId as string);
    return evolution?.transitions ?? [];
  }

  /**
   * Get context transitions between specific nodes
   *
   * @param executionId Workflow execution ID
   * @param fromNodeId Source node
   * @param toNodeId Target node
   * @returns Transitions between nodes
   */
  async getNodeTransitions(
    executionId: ID,
    fromNodeId?: string,
    toNodeId?: string,
  ): Promise<ContextStateTransition[]> {
    const evolution = this.contextEvolutions.get(executionId as string);
    if (!evolution) return [];

    return evolution.transitions.filter(t => {
      if (fromNodeId && t.fromNode !== fromNodeId) return false;
      if (toNodeId && t.toNode !== toNodeId) return false;
      return true;
    });
  }

  // ============================================================================
  // Statistics and Analysis
  // ============================================================================

  /**
   * Get variable mutation count
   *
   * @param executionId Workflow execution ID
   * @returns Total number of variable changes
   */
  async getVariableMutationCount(executionId: ID): Promise<number> {
    const evolution = this.contextEvolutions.get(executionId as string);
    return evolution?.totalVariableChanges ?? 0;
  }

  /**
   * Get most frequently changed variables
   *
   * @param executionId Workflow execution ID
   * @param limit Number of variables to return
   * @returns Variables sorted by change frequency
   */
  async getMostChangedVariables(executionId: ID, limit: number = 10): Promise<string[]> {
    const histories = await this.getAllVariableHistories(executionId);
    const changeCount = new Map<string, number>();

    for (const history of histories) {
      changeCount.set(history.variableName, history.totalChanges);
    }

    return Array.from(changeCount.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, limit)
      .map(([name]) => name);
  }

  /**
   * Get context snapshots at key execution points
   *
   * @param executionId Workflow execution ID
   * @returns Key snapshots
   */
  async getKeyContextSnapshots(executionId: ID): Promise<ExecutionContextSnapshot[]> {
    const evolution = this.contextEvolutions.get(executionId as string);
    return evolution?.keySnapshots ?? [];
  }
}
