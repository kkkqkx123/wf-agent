/**
 * Static Node Type Definitions
 * 
 * These types represent nodes as defined in workflow TOML configuration files.
 * Used during:
 * - Workflow registration and validation
 * - Graph preprocessing (before subgraph expansion)
 * 
 * IMPORTANT: SUBGRAPH nodes exist ONLY in static definitions.
 * After preprocessing, they are expanded and replaced with their internal nodes.
 */

import type { NodeIdentity, StaticNodeDisplayProps, NodeExecutionConfig } from "./shared-node-types.js";

// Import all node configuration types
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
import type { SubgraphNodeConfig } from "./configs/subgraph-configs.js";
import type { AgentLoopNodeConfig } from "./configs/agent-loop-configs.js";
// Import boundary configs for START/END/SUBGRAPH_START and trigger nodes
import type { WorkflowStartConfig, WorkflowEndConfig } from "../workflow/boundary-config.js";
import type { EmbedGraphNodeConfig } from "./configs/embed-graph-configs.js";

// ============================================================================
// Static Node Types
// ============================================================================

export type StaticNodeType =
  | "START"
  | "END"
  | "VARIABLE"
  | "FORK"
  | "JOIN"
  | "SUBGRAPH"  // Creates independent execution entity (Phase 1: Scheme C)
  | "EMBED_GRAPH"  // Lightweight graph expansion for control flow reuse (Phase 3)
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
  | "CONTINUE_FROM_TRIGGER";

// ============================================================================
// Base Static Node Structure
// ============================================================================

/**
 * Base structure for static nodes (as defined in TOML)
 * Combines identity + display metadata + execution config
 */
export interface BaseStaticNode extends NodeIdentity, StaticNodeDisplayProps, NodeExecutionConfig {
  /** Node type discriminator */
  type: StaticNodeType;
}

// ============================================================================
// Static Node Configuration Map
// ============================================================================

export interface StaticNodeConfigMap {
  START: WorkflowStartConfig;
  END: WorkflowEndConfig;
  VARIABLE: VariableNodeConfig;
  FORK: ForkNodeConfig;
  JOIN: JoinNodeConfig;
  SUBGRAPH: SubgraphNodeConfig;
  EMBED_GRAPH: EmbedGraphNodeConfig;
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
}

export type StaticNodeOfType<T extends StaticNodeType> = BaseStaticNode & {
  type: T;
  config: StaticNodeConfigMap[T];
};

// ============================================================================
// Specific Static Node Type Definitions
// ============================================================================

export type StartNode = StaticNodeOfType<"START">;
export type EndNode = StaticNodeOfType<"END">;
export type VariableNode = StaticNodeOfType<"VARIABLE">;
export type ForkNode = StaticNodeOfType<"FORK">;
export type JoinNode = StaticNodeOfType<"JOIN">;
export type SubgraphNode = StaticNodeOfType<"SUBGRAPH">;
export type EmbedGraphNode = StaticNodeOfType<"EMBED_GRAPH">;
export type ScriptNode = StaticNodeOfType<"SCRIPT">;
export type LLMNode = StaticNodeOfType<"LLM">;
export type AddToolNode = StaticNodeOfType<"ADD_TOOL">;
export type UserInteractionNode = StaticNodeOfType<"USER_INTERACTION">;
export type RouteNode = StaticNodeOfType<"ROUTE">;
export type ContextProcessorNode = StaticNodeOfType<"CONTEXT_PROCESSOR">;
export type LoopStartNode = StaticNodeOfType<"LOOP_START">;
export type LoopEndNode = StaticNodeOfType<"LOOP_END">;
export type AgentLoopNode = StaticNodeOfType<"AGENT_LOOP">;
export type StartFromTriggerNode = StaticNodeOfType<"START_FROM_TRIGGER">;
export type ContinueFromTriggerNode = StaticNodeOfType<"CONTINUE_FROM_TRIGGER">;

// ============================================================================
// Static Node Union Type
// ============================================================================

/**
 * Union of all static node types - used for workflow validation and preprocessing
 */
export type StaticNode =
  | StartNode
  | EndNode
  | VariableNode
  | ForkNode
  | JoinNode
  | SubgraphNode
  | EmbedGraphNode
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
  | ContinueFromTriggerNode;

// ============================================================================
// Type Guard Functions
// ============================================================================

export function createStaticNodeTypeGuard<T extends StaticNodeType>(type: T) {
  return (node: unknown): node is StaticNodeOfType<T> =>
    typeof node === "object" && node !== null && (node as StaticNode).type === type;
}

export const isStartNode = createStaticNodeTypeGuard("START");
export const isEndNode = createStaticNodeTypeGuard("END");
export const isVariableNode = createStaticNodeTypeGuard("VARIABLE");
export const isForkNode = createStaticNodeTypeGuard("FORK");
export const isJoinNode = createStaticNodeTypeGuard("JOIN");
export const isSubgraphNode = createStaticNodeTypeGuard("SUBGRAPH");
export const isEmbedGraphNode = createStaticNodeTypeGuard("EMBED_GRAPH");
export const isScriptNode = createStaticNodeTypeGuard("SCRIPT");
export const isLLMNode = createStaticNodeTypeGuard("LLM");
export const isAddToolNode = createStaticNodeTypeGuard("ADD_TOOL");
export const isUserInteractionNode = createStaticNodeTypeGuard("USER_INTERACTION");
export const isRouteNode = createStaticNodeTypeGuard("ROUTE");
export const isContextProcessorNode = createStaticNodeTypeGuard("CONTEXT_PROCESSOR");
export const isLoopStartNode = createStaticNodeTypeGuard("LOOP_START");
export const isLoopEndNode = createStaticNodeTypeGuard("LOOP_END");
export const isAgentLoopNode = createStaticNodeTypeGuard("AGENT_LOOP");
export const isStartFromTriggerNode = createStaticNodeTypeGuard("START_FROM_TRIGGER");
export const isContinueFromTriggerNode = createStaticNodeTypeGuard("CONTINUE_FROM_TRIGGER");
