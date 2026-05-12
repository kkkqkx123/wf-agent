/**
 * Runtime Node Type Definitions
 * 
 * These types represent nodes in the execution graph AFTER preprocessing.
 * Used during:
 * - Workflow execution
 * - Runtime node handling
 * 
 * Key differences from StaticNode:
 * 1. SUBGRAPH nodes are EXPANDED and replaced with their internal structure
 * 2. START/END nodes inside subgraphs become SUBGRAPH_START/SUBGRAPH_END
 * 3. All nodes gain runtime-specific context properties
 */

import type { ID } from "../common.js";
import type { NodeIdentity, NodeExecutionConfig, RuntimeNodeContext } from "./shared-node-types.js";

// Import configuration types (most are shared between static and runtime)
import type { StartNodeConfig, EndNodeConfig, RouteNodeConfig } from "./configs/control-configs.js";
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
import type {
  StartFromTriggerNodeConfig,
  ContinueFromTriggerNodeConfig,
} from "./configs/trigger-subworkflow-configs.js";
import type { AgentLoopNodeConfig } from "./configs/agent-loop-configs.js";

// Import runtime-only configurations (generated during preprocessing)
import type {
  SubgraphStartNodeConfig,
  SubgraphEndNodeConfig,
} from "./runtime/subgraph-runtime-configs.js";

// ============================================================================
// Runtime Node Types
// ============================================================================

/**
 * Node types that exist in the runtime execution graph (after preprocessing)
 * 
 * Note: SUBGRAPH type does NOT exist at runtime - it's expanded during preprocessing
 */
export type RuntimeNodeType =
  | "START"
  | "END"
  | "VARIABLE"
  | "FORK"
  | "JOIN"
  // SUBGRAPH is removed - expanded during preprocessing
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
  // New runtime-only types (replacing START/END in expanded subgraphs)
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
  START: StartNodeConfig;
  END: EndNodeConfig;
  VARIABLE: VariableNodeConfig;
  FORK: ForkNodeConfig;
  JOIN: JoinNodeConfig;
  // SUBGRAPH is removed - doesn't exist at runtime
  SCRIPT: ScriptNodeConfig;
  LLM: LLMNodeConfig;
  ADD_TOOL: AddToolNodeConfig;
  USER_INTERACTION: UserInteractionNodeConfig;
  ROUTE: RouteNodeConfig;
  CONTEXT_PROCESSOR: ContextProcessorNodeConfig;
  LOOP_START: LoopStartNodeConfig;
  LOOP_END: LoopEndNodeConfig;
  AGENT_LOOP: AgentLoopNodeConfig;
  START_FROM_TRIGGER: StartFromTriggerNodeConfig;
  CONTINUE_FROM_TRIGGER: ContinueFromTriggerNodeConfig;
  // Runtime-only node types
  SUBGRAPH_START: SubgraphStartNodeConfig;
  SUBGRAPH_END: SubgraphEndNodeConfig;
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
