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
  hooks?: unknown[];
  
  /** Whether to create checkpoint before node execution */
  checkpointBeforeExecute?: boolean;
  
  /** Whether to create checkpoint after node execution */
  checkpointAfterExecute?: boolean;
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
