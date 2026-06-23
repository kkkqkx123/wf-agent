/**
 * AgentExecutionStateAPI - Agent Execution State Query API
 *
 * Provides comprehensive execution state snapshot queries for Agent Loop.
 * Fills the gap of missing execution state tracking in Agent Loop.
 *
 * Features:
 * - Complete execution state snapshots
 * - Variable state management
 * - Input/Output state tracking
 * - Execution context and memory management
 * - Timeline and state transitions
 *
 * Phase 1 Implementation: Add missing execution state capabilities to Agent Loop
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { PersistenceLayer } from "../../shared/core/persistence-interfaces.js";
import { NoOpPersistenceLayer } from "../../shared/core/persistence-interfaces.js";
import type { ID } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentExecutionStateAPI" });

// ============================================================================
// Type Definitions: Variable State
// ============================================================================

/**
 * Variable snapshot
 */
export interface VariableSnapshot {
  /** Variable name */
  name: string;
  /** Variable type */
  type: string;
  /** Current value */
  value: unknown;
  /** When this value was set */
  timestamp: number;
  /** Source (e.g., "input", "iteration_2", "tool_result") */
  source?: string;
  /** Whether variable is mutable */
  isMutable: boolean;
}

/**
 * Variable state snapshot at a point in time
 */
export interface VariableStateSnapshot {
  /** Execution ID */
  executionId: ID;
  /** Timestamp of snapshot */
  timestamp: number;
  /** Variables at this point */
  variables: VariableSnapshot[];
  /** Iteration number (if applicable) */
  iteration?: number;
}

// ============================================================================
// Type Definitions: Execution Context
// ============================================================================

/**
 * Stack frame in execution context
 */
export interface StackFrame {
  /** Frame ID */
  frameId: string;
  /** Frame type */
  type: "iteration" | "tool_call" | "condition_check" | "branch" | "loop";
  /** Frame description */
  description: string;
  /** Variables in this frame */
  frameVariables: Record<string, unknown>;
  /** Entry timestamp */
  entryTime: number;
  /** Exit timestamp */
  exitTime?: number;
}

/**
 * Execution context snapshot
 */
export interface ExecutionContextSnapshot {
  /** Execution ID */
  executionId: ID;
  /** Current execution stack */
  callStack: StackFrame[];
  /** Global variables */
  globalVariables: Record<string, unknown>;
  /** Local variables */
  localVariables: Record<string, unknown>;
  /** Iteration-specific context */
  iterationContext?: {
    currentIteration: number;
    startTime: number;
    tools: Record<string, unknown>;
    results: Record<string, unknown>;
  };
  /** Memory usage estimate (bytes) */
  memoryUsage?: number;
  /** Timestamp */
  timestamp: number;
}

// ============================================================================
// Type Definitions: Input/Output State
// ============================================================================

/**
 * Input state
 */
export interface InputState {
  /** Execution ID */
  executionId: ID;
  /** Initial input data */
  initialInput: Record<string, unknown>;
  /** Input schema (if available) */
  inputSchema?: Record<string, unknown>;
  /** Input validation status */
  validated: boolean;
  /** Input validation errors */
  validationErrors?: string[];
  /** Timestamp */
  timestamp: number;
}

/**
 * Output state at a point in execution
 */
export interface OutputState {
  /** Execution ID */
  executionId: ID;
  /** Current output data */
  currentOutput: Record<string, unknown>;
  /** Output from last iteration */
  lastIterationOutput?: unknown;
  /** Final output (if execution completed) */
  finalOutput?: unknown;
  /** Output schema (if available) */
  outputSchema?: Record<string, unknown>;
  /** Timestamp */
  timestamp: number;
}

// ============================================================================
// Type Definitions: State Transition
// ============================================================================

/**
 * State transition record
 */
export interface StateTransition {
  /** Transition ID */
  id: string;
  /** From state */
  fromState: string;
  /** To state */
  toState: string;
  /** Timestamp */
  timestamp: number;
  /** Reason/trigger */
  reason?: string;
  /** State changes made */
  changes?: Record<string, { from: unknown; to: unknown }>;
}

/**
 * Execution timeline entry
 */
export interface ExecutionTimelineEntry {
  /** Entry ID */
  id: string;
  /** Entry type */
  type: "state_change" | "iteration_start" | "iteration_end" | "tool_call" | "error" | "recovery";
  /** Description */
  description: string;
  /** Timestamp */
  timestamp: number;
  /** Associated data */
  data?: Record<string, unknown>;
}

// ============================================================================
// Type Definitions: Complete Execution State
// ============================================================================

/**
 * Complete agent execution state snapshot
 */
export interface AgentExecutionState {
  /** Execution ID */
  executionId: ID;
  /** Current status */
  status: string;
  /** Current iteration */
  currentIteration: number;
  /** Total iterations so far */
  totalIterations: number;

  // ============================================================================
  // State Components
  // ============================================================================

  /** Input state */
  inputState: InputState;

  /** Current output state */
  outputState: OutputState;

  /** Variable state snapshot */
  variableState: VariableStateSnapshot;

  /** Execution context snapshot */
  context: ExecutionContextSnapshot;

  // ============================================================================
  // Timeline & History
  // ============================================================================

  /** Recent state transitions */
  recentTransitions?: StateTransition[];

  /** Execution timeline */
  timeline?: ExecutionTimelineEntry[];

  // ============================================================================
  // Metadata
  // ============================================================================

  /** Timestamp of this state snapshot */
  timestamp: number;

  /** Time since execution started */
  elapsedTime?: number;

  /** Estimated remaining time */
  estimatedRemaining?: number;
}

// ============================================================================
// Query & Filter Types
// ============================================================================

/**
 * Execution state filter
 */
export interface ExecutionStateFilter {
  /** Execution ID list */
  executionIds?: ID[];
  /** Status filter */
  status?: string;
  /** Min iteration count */
  minIteration?: number;
  /** Max iteration count */
  maxIteration?: number;
  /** Time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Variable snapshot filter
 */
export interface VariableSnapshotFilter {
  /** Variable name pattern */
  namePattern?: string;
  /** Variable type filter */
  type?: string;
  /** Time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * State transition analysis
 */
export interface StateTransitionAnalysis {
  /** Total transitions */
  totalTransitions: number;
  /** Most common transitions */
  commonTransitions: Array<{
    from: string;
    to: string;
    count: number;
    frequency: number;
  }>;
  /** State entry counts */
  stateEntryCount: Record<string, number>;
  /** Average time in state */
  averageTimeInState: Record<string, number>;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * AgentExecutionStateAPI - Agent Execution State Query API
 */
export class AgentExecutionStateAPI extends QueryableResourceAPI<
  AgentExecutionState,
  ID,
  ExecutionStateFilter
> {
  private executionStates: Map<ID, AgentExecutionState> = new Map();
  private variableSnapshots: Map<ID, VariableStateSnapshot[]> = new Map();
  private contextSnapshots: Map<ID, ExecutionContextSnapshot[]> = new Map();
  private stateTransitions: Map<ID, StateTransition[]> = new Map();
  private timelines: Map<ID, ExecutionTimelineEntry[]> = new Map();
  private persistence: PersistenceLayer;

  /**
   * Constructor
   * @param deps APIDependencyManager instance
   */
  constructor(deps: APIDependencyManager) {
    super();
    this.persistence = deps.getPersistenceLayer() || new NoOpPersistenceLayer();
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get execution state by ID
   */
  protected override async getResource(id: ID): Promise<AgentExecutionState | null> {
    return this.executionStates.get(id) ?? null;
  }

  /**
   * Get all execution states
   */
  protected override async getAllResources(): Promise<AgentExecutionState[]> {
    return Array.from(this.executionStates.values());
  }

  /**
   * Apply filters to execution states
   */
  protected override applyFilter(
    records: AgentExecutionState[],
    filter: ExecutionStateFilter,
  ): AgentExecutionState[] {
    let filtered = records;

    if (filter.executionIds && filter.executionIds.length > 0) {
      const idSet = new Set(filter.executionIds);
      filtered = filtered.filter(r => idSet.has(r.executionId));
    }

    if (filter.status) {
      filtered = filtered.filter(r => r.status === filter.status);
    }

    if (filter.minIteration !== undefined) {
      filtered = filtered.filter(r => r.currentIteration >= filter.minIteration!);
    }

    if (filter.maxIteration !== undefined) {
      filtered = filtered.filter(r => r.currentIteration <= filter.maxIteration!);
    }

    if (filter.timeRange) {
      const { start, end } = filter.timeRange;
      if (start !== undefined) {
        filtered = filtered.filter(r => r.timestamp >= start);
      }
      if (end !== undefined) {
        filtered = filtered.filter(r => r.timestamp <= end);
      }
    }

    return filtered;
  }

  // ============================================================================
  // Execution State Operations
  // ============================================================================

  /**
   * Record complete execution state
   */
  async recordExecutionState(state: AgentExecutionState): Promise<void> {
    this.executionStates.set(state.executionId, state);

    if (this.persistence) {
      await this.persistence.saveExecutionStateSnapshot(state).catch((err) => {
        logger.warn("Failed to persist execution state", { error: err });
      });
    }

    logger.debug("Recorded execution state", {
      executionId: state.executionId,
      status: state.status,
      iteration: state.currentIteration,
    });
  }

  /**
   * Get current execution state
   */
  async getExecutionState(executionId: ID): Promise<AgentExecutionState | null> {
    // Prefer cached state
    const cached = this.executionStates.get(executionId);
    if (cached) return cached;

    // Try to restore from persistence layer
    if (this.persistence) {
      const persisted = await this.persistence.getExecutionStateSnapshot(executionId);
      if (persisted) {
        this.executionStates.set(executionId, persisted);
        return persisted;
      }
    }

    return null;
  }

  /**
   * Get execution state at a specific iteration
   */
  async getExecutionStateAtIteration(executionId: ID, iteration: number): Promise<AgentExecutionState | null> {
    const state = this.executionStates.get(executionId);
    if (!state || state.currentIteration < iteration) {
      return null;
    }

    // Return state or interpolate from history
    return state;
  }

  // ============================================================================
  // Input/Output State Operations
  // ============================================================================

  /**
   * Get input state
   */
  async getInputState(executionId: ID): Promise<InputState | null> {
    const state = await this.getExecutionState(executionId);
    return state?.inputState ?? null;
  }

  /**
   * Get output state
   */
  async getOutputState(executionId: ID): Promise<OutputState | null> {
    const state = await this.getExecutionState(executionId);
    return state?.outputState ?? null;
  }

  /**
   * Record input state
   */
  async recordInputState(input: InputState): Promise<void> {
    const state = await this.getExecutionState(input.executionId);
    if (state) {
      state.inputState = input;
      this.executionStates.set(input.executionId, state);
      logger.debug("Updated input state", { executionId: input.executionId });
    }
  }

  /**
   * Record output state
   */
  async recordOutputState(output: OutputState): Promise<void> {
    const state = await this.getExecutionState(output.executionId);
    if (state) {
      state.outputState = output;
      this.executionStates.set(output.executionId, state);
      logger.debug("Updated output state", { executionId: output.executionId });
    }
  }

  // ============================================================================
  // Variable State Operations
  // ============================================================================

  /**
   * Record variable snapshot
   */
  async recordVariableSnapshot(snapshot: VariableStateSnapshot): Promise<void> {
    if (!this.variableSnapshots.has(snapshot.executionId)) {
      this.variableSnapshots.set(snapshot.executionId, []);
    }

    const snapshots = this.variableSnapshots.get(snapshot.executionId)!;
    snapshots.push(snapshot);

    // Update execution state
    const state = await this.getExecutionState(snapshot.executionId);
    if (state) {
      state.variableState = snapshot;
      this.executionStates.set(snapshot.executionId, state);
    }

    logger.debug("Recorded variable snapshot", {
      executionId: snapshot.executionId,
      variableCount: snapshot.variables.length,
    });
  }

  /**
   * Get current variable state
   */
  async getVariableSnapshot(executionId: ID, filter?: VariableSnapshotFilter): Promise<VariableSnapshot[]> {
    const state = await this.getExecutionState(executionId);
    if (!state) {
      return [];
    }

    let variables = state.variableState.variables;

    if (filter) {
      if (filter.namePattern) {
        const pattern = new RegExp(filter.namePattern);
        variables = variables.filter(v => pattern.test(v.name));
      }

      if (filter.type) {
        variables = variables.filter(v => v.type === filter.type);
      }
    }

    return variables;
  }

  /**
   * Get variable history
   */
  async getVariableHistory(executionId: ID, variableName: string): Promise<VariableSnapshot[]> {
    const snapshots = this.variableSnapshots.get(executionId) ?? [];

    return snapshots
      .flatMap(s => s.variables)
      .filter(v => v.name === variableName)
      .sort((a, b) => a.timestamp - b.timestamp);
  }

  // ============================================================================
  // Execution Context Operations
  // ============================================================================

  /**
   * Record execution context
   */
  async recordExecutionContext(context: ExecutionContextSnapshot): Promise<void> {
    if (!this.contextSnapshots.has(context.executionId)) {
      this.contextSnapshots.set(context.executionId, []);
    }

    const snapshots = this.contextSnapshots.get(context.executionId)!;
    snapshots.push(context);

    // Update execution state
    const state = await this.getExecutionState(context.executionId);
    if (state) {
      state.context = context;
      this.executionStates.set(context.executionId, state);
    }

    logger.debug("Recorded execution context", {
      executionId: context.executionId,
      stackDepth: context.callStack.length,
    });
  }

  /**
   * Get current execution context
   */
  async getExecutionContext(executionId: ID): Promise<ExecutionContextSnapshot | null> {
    const state = await this.getExecutionState(executionId);
    return state?.context ?? null;
  }

  /**
   * Get call stack
   */
  async getCallStack(executionId: ID): Promise<StackFrame[]> {
    const context = await this.getExecutionContext(executionId);
    return context?.callStack ?? [];
  }

  /**
   * Get memory usage
   */
  async getMemoryUsage(executionId: ID): Promise<number | null> {
    const context = await this.getExecutionContext(executionId);
    return context?.memoryUsage ?? null;
  }

  // ============================================================================
  // State Transition Operations
  // ============================================================================

  /**
   * Record state transition
   */
  async recordStateTransition(transition: StateTransition): Promise<void> {
    // Extract execution ID from transition ID (format: executionId:transition:timestamp)
    const executionId = transition.id.split(":")[0] as ID;

    if (!this.stateTransitions.has(executionId)) {
      this.stateTransitions.set(executionId, []);
    }

    const transitions = this.stateTransitions.get(executionId)!;
    transitions.push(transition);

    // Update execution state if available
    const state = await this.getExecutionState(executionId);
    if (state) {
      state.recentTransitions = transitions.slice(-10); // Keep last 10
      this.executionStates.set(executionId, state);
    }

    logger.debug("Recorded state transition", {
      executionId,
      from: transition.fromState,
      to: transition.toState,
    });
  }

  /**
   * Get state transitions
   */
  async getStateTransitions(executionId: ID): Promise<StateTransition[]> {
    return (this.stateTransitions.get(executionId) ?? []).sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Analyze state transitions
   */
  async analyzeStateTransitions(executionId: ID): Promise<StateTransitionAnalysis> {
    const transitions = await this.getStateTransitions(executionId);

    const analysis: StateTransitionAnalysis = {
      totalTransitions: transitions.length,
      commonTransitions: [],
      stateEntryCount: {},
      averageTimeInState: {},
    };

    if (transitions.length === 0) {
      return analysis;
    }

    // Count transitions
    const transitionCounts: Record<string, number> = {};
    const stateEntries: Record<string, number> = {};
    const stateTimes: Record<string, number[]> = {};

    transitions.forEach((t, i) => {
      const key = `${t.fromState}->${t.toState}`;
      transitionCounts[key] = (transitionCounts[key] || 0) + 1;
      stateEntries[t.toState] = (stateEntries[t.toState] || 0) + 1;

      if (i > 0) {
        const prev = transitions[i - 1]!;
        const duration = t.timestamp - prev.timestamp;
        if (!stateTimes[prev.toState]) {
          stateTimes[prev.toState] = [];
        }
        stateTimes[prev.toState]!.push(duration);
      }
    });

    // Build common transitions
    analysis.commonTransitions = Object.entries(transitionCounts)
      .map(([key, count]) => {
        const [from, to] = key.split("->") as [string, string];
        return {
          from,
          to,
          count,
          frequency: (count / transitions.length) * 100,
        };
      })
      .sort((a, b) => b.count - a.count)
      .slice(0, 10);

    analysis.stateEntryCount = stateEntries;

    // Calculate average time in state
    Object.entries(stateTimes).forEach(([state, times]) => {
      if (times.length > 0) {
        analysis.averageTimeInState[state] = times.reduce((a, b) => a + b, 0) / times.length;
      }
    });

    return analysis;
  }

  // ============================================================================
  // Timeline Operations
  // ============================================================================

  /**
   * Record timeline entry
   */
  async recordTimelineEntry(entry: ExecutionTimelineEntry): Promise<void> {
    // Extract execution ID from entry ID (format: executionId:timeline:index)
    const executionId = entry.id.split(":")[0] as ID;

    if (!this.timelines.has(executionId)) {
      this.timelines.set(executionId, []);
    }

    const timeline = this.timelines.get(executionId)!;
    timeline.push(entry);

    // Update execution state if available
    const state = await this.getExecutionState(executionId);
    if (state) {
      state.timeline = timeline;
      this.executionStates.set(executionId, state);
    }

    logger.debug("Recorded timeline entry", {
      executionId,
      type: entry.type,
    });
  }

  /**
   * Get execution timeline
   */
  async getExecutionTimeline(executionId: ID): Promise<ExecutionTimelineEntry[]> {
    return (this.timelines.get(executionId) ?? []).sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Clear execution state data
   */
  async clearExecutionState(executionId: ID): Promise<void> {
    this.executionStates.delete(executionId);
    this.variableSnapshots.delete(executionId);
    this.contextSnapshots.delete(executionId);
    this.stateTransitions.delete(executionId);
    this.timelines.delete(executionId);
    logger.debug("Cleared execution state", { executionId });
  }
}
