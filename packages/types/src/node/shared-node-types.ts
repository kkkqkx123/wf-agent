/**
 * Shared Node Base Types
 *
 * Common type definitions shared between static and runtime node types.
 * This file contains the foundational types that both static and runtime nodes inherit from.
 */

import type { ID } from "../common.js";

// ============================================================================
// Minimal Node Identity (shared by both static and runtime)
// ============================================================================

/**
 * Minimal node identity - the absolute minimum required to identify a node
 */
export interface NodeIdentity {
  /** Node Unique Identifier */
  id: ID;

  /** Node type discriminator */
  type: string;
}

// ============================================================================
// Static-Only Properties (for CRUD, display, documentation)
// ============================================================================

/**
 * Properties that only exist in static node definitions
 * Used for: workflow editing, UI display, filtering, documentation
 * NOT used during runtime execution
 */
export interface StaticNodeDisplayProps {
  /** Human-readable node name for UI display */
  name: string;

  /** Optional description for documentation/UI tooltips */
  description?: string;

  /** User-defined metadata (for organization, tagging, search filtering) */
  metadata?: Record<string, unknown>;
}

// ============================================================================
// Runtime Execution Configuration (used by execution engine)
// ============================================================================

/**
 * Execution behavior configuration
 * These properties are read by the execution engine at runtime
 * They can be defined statically but are consumed dynamically
 */
export interface NodeExecutionConfig {
  /** Hook configurations for lifecycle events */
  hooks?: import("./hooks.js").NodeHook[];

  /** Whether to create checkpoint before node execution */
  checkpointBeforeExecute?: boolean;

  /** Whether to create checkpoint after node execution */
  checkpointAfterExecute?: boolean;

  /**
   * Semantic output ID for referencing this node's output in expressions.
   * Used in condition expressions like: node.<outputId>.field
   */
  outputId?: string;

  /**
   * Failure handling strategy for node execution.
   * - 'fail': Immediately fail the workflow (default for most nodes).
   * - 'retry': Retry the node execution up to maxRetries times.
   * - 'continue': Return a fallback result and continue execution.
   * Not applicable to control-flow nodes (START, END, FORK, JOIN, SYNC, ROUTE, LOOP_START, LOOP_END).
   * @default 'fail' (LLM and AGENT_LOOP default to 'retry' with maxRetries=3)
   */
  onFailure?: 'fail' | 'retry' | 'continue';

  /** Maximum number of retry attempts (only used when onFailure is 'retry'). @default 3 */
  maxRetries?: number;

  /** Base delay between retries in milliseconds. @default 1000 */
  retryDelayMs?: number;

  /** Whether to use exponential backoff for retry delays. @default true */
  exponentialBackoff?: boolean;

  /**
   * Fallback output value when continuing on failure.
   * Only used when onFailure is 'continue'.
   */
  fallbackOutput?: Record<string, unknown>;
}

// ============================================================================
// Runtime-Only Context (injected during preprocessing)
// ============================================================================

/**
 * Runtime context properties injected during graph preprocessing
 * These do NOT exist in static definitions
 */
export interface RuntimeNodeContext {
  /** Internal system metadata (not user-defined, used for execution tracing) */
  internalMetadata?: Record<string, unknown>;

  /** Reference back to the original static node definition */
  originalNode?: unknown;

  /** Workflow context information */
  workflowId: ID;
  parentWorkflowId?: ID;

  /** Graph structure (populated during preprocessing) */
  outgoingEdgeIds: ID[];
  incomingEdgeIds: ID[];
}
