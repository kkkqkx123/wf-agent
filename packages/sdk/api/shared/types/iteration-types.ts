/**
 * Shared Iteration Analysis Types
 *
 * Common type definitions used across Workflow and Agent iteration analysis APIs.
 * These types are shared to ensure consistency between the two execution models.
 */

// ============================================================================
// Tool Dependency
// ============================================================================

/**
 * Tool dependency relationship between tool calls
 */
export interface ToolDependency {
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** IDs of tools this call depends on */
  dependsOn: string[];
  /** Dependency type */
  dependencyType: "sequential" | "parallel" | "conditional" | "result-dependent";
  /** Why this dependency exists */
  reason?: string;
}

// ============================================================================
// Execution Path
// ============================================================================

/**
 * Execution path step
 */
export interface ExecutionPathStep {
  stepId: string;
  type: "node_execution" | "condition_check" | "branch_decision" | "tool_call" | "decision" | "action";
  description: string;
  result?: unknown;
  timestamp: number;
  duration?: number;
}

/**
 * Base execution path step (alias for ExecutionPathStep for clarity in extensions)
 */
export type BaseExecutionPathStep = ExecutionPathStep;

/**
 * Complete execution path through the system
 */
export interface ExecutionPath {
  /** Unique path identifier */
  pathId: string;
  /** Path description */
  description: string;
  /** Sequence of steps/decisions taken */
  steps: ExecutionPathStep[];
  /** Whether this path was optimal */
  isOptimal?: boolean;
  /** Alternative paths that were considered */
  alternativePaths?: string[];
}

// ============================================================================
// LLM Reasoning Record
// ============================================================================

/**
 * Record of LLM reasoning step
 */
export interface LLMReasoningRecord {
  /** Reasoning step identifier */
  stepId: string;
  /** Type of reasoning (thinking, planning, analyzing, evaluating) */
  reasoningType: "thinking" | "planning" | "analyzing" | "evaluating" | "synthesizing";
  /** Reasoning content */
  content: string;
  /** Confidence in this reasoning */
  confidence?: number;
  /** Related entities (e.g., tools, data sources) */
  relatedEntities?: string[];
  /** Conclusions drawn */
  conclusions?: string[];
}

// ============================================================================
// Optimization Opportunity (base)
// ============================================================================

/**
 * Base optimization opportunity
 * Extended by domain-specific versions in Workflow and Agent APIs
 */
export interface BaseOptimizationOpportunity {
  /** Opportunity description */
  description: string;
  /** Impact level */
  impactLevel: "low" | "medium" | "high";
  /** Estimated improvement description */
  estimatedImprovement?: string;
}