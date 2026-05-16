/**
 * Runtime Node Type Definitions
 * 
 * These types represent nodes in the execution graph AFTER preprocessing.
 * Used during:
 * - Workflow execution
 * - Runtime node handling
 * 
 * Key differences from StaticNode:
 * 1. SUBGRAPH nodes are NOT expanded (Phase 1: Scheme C) - executed as independent entities
 * 2. EMBED_GRAPH nodes ARE expanded (Phase 3) - their START/END become SUBGRAPH_START/SUBGRAPH_END
 * 3. All nodes gain runtime-specific context properties
 * 
 * IMPORTANT: SUBGRAPH_START/SUBGRAPH_END are INTERNAL types used ONLY for EMBED_GRAPH expansion.
 * They should NOT be used directly in workflow definitions.
 */

import type { NodeIdentity, NodeExecutionConfig, RuntimeNodeContext } from "./shared-node-types.js";

// Import configuration types (most are shared between static and runtime)
import type { RouteNodeConfig } from "./configs/control-configs.js";
import type { VariableNodeConfig } from "./configs/variable-configs.js";
import type { ForkNodeConfig, JoinNodeConfig } from "./configs/fork-join-configs.js";
import type { LoopStartNodeConfig, LoopEndNodeConfig } from "./configs/loop-configs.js";
import type {
  ScriptNodeConfig,
  LLMNodeConfig,
  AddToolNodeConfig,
} from "./configs/execution-configs.js";
import type { UserInteractionNodeConfig } from "./configs/interaction-configs.js";
import type { ContextProcessorNodeConfig } from "./configs/context-configs.js";
import type { AgentLoopNodeConfig } from "./configs/agent-loop-configs.js";
// Import boundary configs for START/END nodes and trigger nodes
import type { WorkflowStartConfig, WorkflowEndConfig } from "../workflow/boundary-config.js";

// ============================================================================
// Runtime Node Types
// ============================================================================

/**
 * Node types that exist in the runtime execution graph (after preprocessing)
 * 
 * Note: 
 * - SUBGRAPH type does NOT exist at runtime (Phase 1: Scheme C)
 * - EMBED_GRAPH type does NOT exist at runtime (expanded during preprocessing)
 * - SUBGRAPH_START/SUBGRAPH_END are INTERNAL types for EMBED_GRAPH expansion only
 */
export type RuntimeNodeType =
  | "START"
  | "END"
  | "VARIABLE"
  | "FORK"
  | "JOIN"
  // SUBGRAPH is removed - creates independent execution entity at runtime (Phase 1)
  // EMBED_GRAPH is removed - expanded during preprocessing (Phase 3)
  | "SCRIPT"
  | "LLM"
  | "ADD_TOOL"
  | "USER_INTERACTION"
  | "ROUTE"
  | "CONTEXT_PROCESSOR"
  | "LOOP_START"
  | "LOOP_END"
  | "AGENT_LOOP"
  | "START_FROM_TRIGGER"
  | "CONTINUE_FROM_TRIGGER"
  // Internal runtime-only types (used ONLY for EMBED_GRAPH expansion)
  // WARNING: Do not use these types in workflow definitions
  | "SUBGRAPH_START"
  | "SUBGRAPH_END";

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
  // SUBGRAPH doesn't exist at runtime (Phase 1: Scheme C)
  // EMBED_GRAPH doesn't exist at runtime (expanded during preprocessing, Phase 3)
  SCRIPT: ScriptNodeConfig;
  LLM: LLMNodeConfig;
  ADD_TOOL: AddToolNodeConfig;
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
  SUBGRAPH_START: WorkflowStartConfig;
  SUBGRAPH_END: WorkflowEndConfig;
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
export type ScriptNode = RuntimeNodeOfType<"SCRIPT">;
export type LLMNode = RuntimeNodeOfType<"LLM">;
export type AddToolNode = RuntimeNodeOfType<"ADD_TOOL">;
export type UserInteractionNode = RuntimeNodeOfType<"USER_INTERACTION">;
export type RouteNode = RuntimeNodeOfType<"ROUTE">;
export type ContextProcessorNode = RuntimeNodeOfType<"CONTEXT_PROCESSOR">;
export type LoopStartNode = RuntimeNodeOfType<"LOOP_START">;
export type LoopEndNode = RuntimeNodeOfType<"LOOP_END">;
export type AgentLoopNode = RuntimeNodeOfType<"AGENT_LOOP">;
export type StartFromTriggerNode = RuntimeNodeOfType<"START_FROM_TRIGGER">;
export type ContinueFromTriggerNode = RuntimeNodeOfType<"CONTINUE_FROM_TRIGGER">;

// Runtime-only node types (generated during subgraph expansion)
export type SubgraphStartNode = RuntimeNodeOfType<"SUBGRAPH_START">;
export type SubgraphEndNode = RuntimeNodeOfType<"SUBGRAPH_END">;

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
  | ScriptNode
  | LLMNode
  | AddToolNode
  | UserInteractionNode
  | RouteNode
  | ContextProcessorNode
  | LoopStartNode
  | LoopEndNode
  | AgentLoopNode
  | StartFromTriggerNode
  | ContinueFromTriggerNode
  | SubgraphStartNode
  | SubgraphEndNode;

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
export const isScriptNode = createRuntimeNodeTypeGuard("SCRIPT");
export const isLLMNode = createRuntimeNodeTypeGuard("LLM");
export const isAddToolNode = createRuntimeNodeTypeGuard("ADD_TOOL");
export const isUserInteractionNode = createRuntimeNodeTypeGuard("USER_INTERACTION");
export const isRouteNode = createRuntimeNodeTypeGuard("ROUTE");
export const isContextProcessorNode = createRuntimeNodeTypeGuard("CONTEXT_PROCESSOR");
export const isLoopStartNode = createRuntimeNodeTypeGuard("LOOP_START");
export const isLoopEndNode = createRuntimeNodeTypeGuard("LOOP_END");
export const isAgentLoopNode = createRuntimeNodeTypeGuard("AGENT_LOOP");
export const isStartFromTriggerNode = createRuntimeNodeTypeGuard("START_FROM_TRIGGER");
export const isContinueFromTriggerNode = createRuntimeNodeTypeGuard("CONTINUE_FROM_TRIGGER");

// Runtime-only type guards
export const isSubgraphStartNode = createRuntimeNodeTypeGuard("SUBGRAPH_START");
export const isSubgraphEndNode = createRuntimeNodeTypeGuard("SUBGRAPH_END");
