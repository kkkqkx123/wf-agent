/**
 * Runtime Node Type Definitions
 * 
 * These types represent nodes in the execution graph AFTER preprocessing.
 * Used during:
 * - Workflow execution
 * - Runtime node handling
 * 
 * Key differences from StaticNode:
 * 1. SUBGRAPH nodes remain in runtime graph (Phase 1: Scheme C) - executed as independent entities
 * 2. EMBED_GRAPH nodes ARE expanded (Phase 3) - their START/END become EMBED_START/EMBED_END
 * 3. All nodes gain runtime-specific context properties
 * 
 * IMPORTANT: EMBED_START/EMBED_END are INTERNAL types used ONLY for EMBED_GRAPH expansion.
 * They should NOT be used directly in workflow definitions.
 */

import type { NodeIdentity, NodeExecutionConfig, RuntimeNodeContext } from "./shared-node-types.js";

// Import configuration types (most are shared between static and runtime)
import type { RouteNodeConfig, RouteNodeOutput } from "./configs/control-configs.js";
import type { VariableNodeConfig, VariableNodeOutput } from "./configs/variable-configs.js";
import type { ForkNodeConfig, ForkNodeOutput, JoinNodeConfig, JoinNodeOutput } from "./configs/fork-join-configs.js";
import type { LoopStartNodeConfig, LoopStartNodeOutput, LoopEndNodeConfig, LoopEndNodeOutput } from "./configs/loop-configs.js";
import type {
  ScriptNodeConfig,
  ScriptNodeOutput,
  LLMNodeConfig,
  LLMNodeOutput,
  ToolVisibilityNodeConfig,
  ToolVisibilityNodeOutput,
} from "./configs/execution-configs.js";
import type { UserInteractionNodeConfig, UserInteractionNodeOutput } from "./configs/interaction-configs.js";
import type { ContextProcessorNodeConfig, ContextProcessorNodeOutput } from "./configs/context-configs.js";
import type { AgentLoopNodeConfig, AgentLoopNodeOutput } from "./configs/agent-loop-configs.js";
import type { SubgraphNodeConfig, SubgraphNodeOutput } from "./configs/subgraph-configs.js";
import type { SyncNodeConfig, SyncNodeOutput } from "./configs/sync-configs.js";
// Import boundary configs for START/END nodes and trigger nodes
import type { WorkflowStartConfig, WorkflowEndConfig, StartNodeOutput, EndNodeOutput } from "../workflow/boundary-config.js";

// ============================================================================
// Runtime Node Types
// ============================================================================

/**
 * Node types that exist in the runtime execution graph (after preprocessing)
 * 
 * Note: 
 * - SUBGRAPH type EXISTS at runtime (Phase 1: Scheme C) - creates independent execution entity
 * - EMBED_GRAPH type does NOT exist at runtime (expanded during preprocessing)
 * - EMBED_START/EMBED_END are INTERNAL types for EMBED_GRAPH expansion only
 */
export type RuntimeNodeType =
  | "START"
  | "END"
  | "VARIABLE"
  | "FORK"
  | "JOIN"
  | "SYNC"  // Explicit synchronization between fork branches
  | "SUBGRAPH"  // Exists at runtime, handled by subgraphHandler (Phase 1: Scheme C)
  // EMBED_GRAPH is removed - expanded during preprocessing (Phase 3)
  | "SCRIPT"
  | "LLM"
  | "TOOL_VISIBILITY"
  | "USER_INTERACTION"
  | "ROUTE"
  | "CONTEXT_PROCESSOR"
  | "LOOP_START"
  | "LOOP_END"
  | "AGENT_LOOP"
  | "START_FROM_TRIGGER"
  | "CONTINUE_FROM_TRIGGER"
  // Internal runtime-only types (used ONLY for EMBED_GRAPH expansion)
  | "EMBED_START"
  | "EMBED_END";

// ============================================================================
// Base Runtime Node Structure
// ============================================================================

/**
 * Base structure for runtime nodes
 * Combines identity + execution config + runtime context
 * Note: Does NOT include display metadata (name/description/metadata) as those are not needed at runtime
 */
export interface BaseRuntimeNode extends NodeIdentity, NodeExecutionConfig, RuntimeNodeContext {
  /** Node type discriminator */
  type: RuntimeNodeType;
}

// ============================================================================
// Runtime Node Configuration Map
// ============================================================================

export interface RuntimeNodeConfigMap {
  START: WorkflowStartConfig;
  END: WorkflowEndConfig;
  VARIABLE: VariableNodeConfig;
  FORK: ForkNodeConfig;
  JOIN: JoinNodeConfig;
  SYNC: SyncNodeConfig;
  SUBGRAPH: SubgraphNodeConfig;
  SCRIPT: ScriptNodeConfig;
  LLM: LLMNodeConfig;
  TOOL_VISIBILITY: ToolVisibilityNodeConfig;
  USER_INTERACTION: UserInteractionNodeConfig;
  ROUTE: RouteNodeConfig;
  CONTEXT_PROCESSOR: ContextProcessorNodeConfig;
  LOOP_START: LoopStartNodeConfig;
  LOOP_END: LoopEndNodeConfig;
  AGENT_LOOP: AgentLoopNodeConfig;
  START_FROM_TRIGGER: WorkflowStartConfig;
  CONTINUE_FROM_TRIGGER: WorkflowEndConfig;
  // Internal runtime-only types (used ONLY for EMBED_GRAPH expansion)
  // WARNING: Do not configure these types manually
  EMBED_START: WorkflowStartConfig;
  EMBED_END: WorkflowEndConfig;
}

export type RuntimeNodeOfType<T extends RuntimeNodeType> = BaseRuntimeNode & {
  type: T;
  config: RuntimeNodeConfigMap[T];
};

// ============================================================================
// Specific Runtime Node Type Definitions
// ============================================================================

export type StartNode = RuntimeNodeOfType<"START">;
export type EndNode = RuntimeNodeOfType<"END">;
export type VariableNode = RuntimeNodeOfType<"VARIABLE">;
export type ForkNode = RuntimeNodeOfType<"FORK">;
export type JoinNode = RuntimeNodeOfType<"JOIN">;
export type SyncNode = RuntimeNodeOfType<"SYNC">;
export type SubgraphNode = RuntimeNodeOfType<"SUBGRAPH">;  // Exists at runtime (Phase 1: Scheme C)
export type ScriptNode = RuntimeNodeOfType<"SCRIPT">;
export type LLMNode = RuntimeNodeOfType<"LLM">;
export type ToolVisibilityNode = RuntimeNodeOfType<"TOOL_VISIBILITY">;
export type UserInteractionNode = RuntimeNodeOfType<"USER_INTERACTION">;
export type RouteNode = RuntimeNodeOfType<"ROUTE">;
export type ContextProcessorNode = RuntimeNodeOfType<"CONTEXT_PROCESSOR">;
export type LoopStartNode = RuntimeNodeOfType<"LOOP_START">;
export type LoopEndNode = RuntimeNodeOfType<"LOOP_END">;
export type AgentLoopNode = RuntimeNodeOfType<"AGENT_LOOP">;
export type StartFromTriggerNode = RuntimeNodeOfType<"START_FROM_TRIGGER">;
export type ContinueFromTriggerNode = RuntimeNodeOfType<"CONTINUE_FROM_TRIGGER">;

// Runtime-only node types (generated during EMBED_GRAPH expansion)
export type EmbedStartNode = RuntimeNodeOfType<"EMBED_START">;
export type EmbedEndNode = RuntimeNodeOfType<"EMBED_END">;

// ============================================================================
// Runtime Node Union Type
// ============================================================================

/**
 * Union of all runtime node types - used during workflow execution
 */
export type RuntimeNode =
  | StartNode
  | EndNode
  | VariableNode
  | ForkNode
  | JoinNode
  | SyncNode
  | SubgraphNode  // Exists at runtime
  | ScriptNode
  | LLMNode
  | ToolVisibilityNode
  | UserInteractionNode
  | RouteNode
  | ContextProcessorNode
  | LoopStartNode
  | LoopEndNode
  | AgentLoopNode
  | StartFromTriggerNode
  | ContinueFromTriggerNode
  | EmbedStartNode
  | EmbedEndNode;

// ============================================================================
// Node Output Type Integration
// ============================================================================

/**
 * Maps each RuntimeNodeType to its output shape.
 * This provides type-safe output access based on node type.
 */
export interface RuntimeNodeOutputMap {
  START: StartNodeOutput;
  END: EndNodeOutput;
  VARIABLE: VariableNodeOutput;
  FORK: ForkNodeOutput;
  JOIN: JoinNodeOutput;
  SYNC: SyncNodeOutput;
  SUBGRAPH: SubgraphNodeOutput;
  SCRIPT: ScriptNodeOutput;
  LLM: LLMNodeOutput;
  TOOL_VISIBILITY: ToolVisibilityNodeOutput;
  USER_INTERACTION: UserInteractionNodeOutput;
  ROUTE: RouteNodeOutput;
  CONTEXT_PROCESSOR: ContextProcessorNodeOutput;
  LOOP_START: LoopStartNodeOutput;
  LOOP_END: LoopEndNodeOutput;
  AGENT_LOOP: AgentLoopNodeOutput;
  START_FROM_TRIGGER: StartNodeOutput;
  CONTINUE_FROM_TRIGGER: EndNodeOutput;
  EMBED_START: StartNodeOutput;
  EMBED_END: EndNodeOutput;
}

/**
 * Helper to extract node output type from RuntimeNodeOutputMap by node type
 */
export type RuntimeNodeOutputOfType<T extends RuntimeNodeType> = RuntimeNodeOutputMap[T];

// ============================================================================
// Type Guard Functions
// ============================================================================

export function createRuntimeNodeTypeGuard<T extends RuntimeNodeType>(type: T) {
  return (node: unknown): node is RuntimeNodeOfType<T> =>
    typeof node === "object" && node !== null && (node as RuntimeNode).type === type;
}

export const isStartNode = createRuntimeNodeTypeGuard("START");
export const isEndNode = createRuntimeNodeTypeGuard("END");
export const isVariableNode = createRuntimeNodeTypeGuard("VARIABLE");
export const isForkNode = createRuntimeNodeTypeGuard("FORK");
export const isJoinNode = createRuntimeNodeTypeGuard("JOIN");
export const isSyncNode = createRuntimeNodeTypeGuard("SYNC");
export const isSubgraphNode = createRuntimeNodeTypeGuard("SUBGRAPH");  // Exists at runtime
export const isScriptNode = createRuntimeNodeTypeGuard("SCRIPT");
export const isLLMNode = createRuntimeNodeTypeGuard("LLM");
export const isToolVisibilityNode = createRuntimeNodeTypeGuard("TOOL_VISIBILITY");
export const isUserInteractionNode = createRuntimeNodeTypeGuard("USER_INTERACTION");
export const isRouteNode = createRuntimeNodeTypeGuard("ROUTE");
export const isContextProcessorNode = createRuntimeNodeTypeGuard("CONTEXT_PROCESSOR");
export const isLoopStartNode = createRuntimeNodeTypeGuard("LOOP_START");
export const isLoopEndNode = createRuntimeNodeTypeGuard("LOOP_END");
export const isAgentLoopNode = createRuntimeNodeTypeGuard("AGENT_LOOP");
export const isStartFromTriggerNode = createRuntimeNodeTypeGuard("START_FROM_TRIGGER");
export const isContinueFromTriggerNode = createRuntimeNodeTypeGuard("CONTINUE_FROM_TRIGGER");

// Runtime-only type guards (for EMBED_GRAPH expansion)
export const isEmbedStartNode = createRuntimeNodeTypeGuard("EMBED_START");
export const isEmbedEndNode = createRuntimeNodeTypeGuard("EMBED_END");
