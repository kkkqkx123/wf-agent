/**
 * Basic Node Type Definition
 * Implementing Type Safety with Discriminated Unions
 */

import type { ID, Metadata } from "../common.js";

// Import all node configuration types
import type { StartNodeConfig, EndNodeConfig, RouteNodeConfig } from "./configs/control-configs.js";

import type { VariableNodeConfig } from "./configs/variable-configs.js";

import type { ForkNodeConfig } from "./configs/fork-join-configs.js";

import type { JoinNodeConfig } from "./configs/fork-join-configs.js";

import type { LoopStartNodeConfig, LoopEndNodeConfig } from "./configs/loop-configs.js";

import type {
  ScriptNodeConfig,
  LLMNodeConfig,
  AddToolNodeConfig,
} from "./configs/execution-configs.js";

import type { UserInteractionNodeConfig } from "./configs/interaction-configs.js";

import type { ContextProcessorNodeConfig } from "./configs/context-configs.js";

import type {
  SubgraphNodeConfig,
  StartFromTriggerNodeConfig,
  ContinueFromTriggerNodeConfig,
} from "./configs/subgraph-configs.js";

import type { AgentLoopNodeConfig } from "./agent-loop.js";

/**
 * Node type
 */
export type NodeType =
  /** Start Node. Serves as a workflow start flag and must be unique. The entry degree must be 0. */
  | "START"
  /** End Node. Serves as a workflow end flag and must be unique. The out degree must be 0. */
  | "END"
  /** Variable Operations node. The primary use is to change the values of workflow variables to provide data for edge condition evaluation. */
  | "VARIABLE"
  /** Fork node. Used to control the fork operation of thread. */
  | "FORK"
  /** Connection node. Used to control the join operation of thread. */
  | "JOIN"
  /** Subgraph node. Used to link to a subworkflow. This node is automatically replaced by merge during the workflow processing phase to connect to the subworkflow with the start node of the subworkflow. */
  | "SUBGRAPH"
  /** Script node. Used to execute scripts. */
  | "SCRIPT"
  /** LLM node. Used to perform LLM api calls. No cue words are added; the context processing node is responsible for cue word operations. */
  | "LLM"
  /** Tool Add Node. Used to dynamically add tools to a tool context. */
  | "ADD_TOOL"
  /** User interaction node. Used to trigger the display front-end user interaction. Only provides input and output channels, not concerned with front-end implementation details. */
  | "USER_INTERACTION"
  /** Routing Node. Used to route to the next node based on conditions. */
  | "ROUTE"
  /** 上下文处理器节点。用于对提示词上下文(消息数组)进行处理。 */
  | "CONTEXT_PROCESSOR"
  /** Loop start node. Marks the start of the loop and sets the loop variables. Loop variables can be modified by VARIABLE nodes. Doesn't care about exit conditions other than the condition */
  | "LOOP_START"
  /** End-of-loop node. Marks the end of the loop. Lets the loop count variable increment itself, and will be displayed depending on whether the loop count reaches the */
  | "LOOP_END"
  /** Agent self-loop node. For simple tasks LLM-Tools self-cycling, or as main coordination engine. For complex control use LOOP_START/LOOP_END + graph orchestration. */
  | "AGENT_LOOP"
  /** The node from which the trigger was started. Identifies the start point of an isolated sub-workflow started by a trigger. No special configuration, similar to the START node. */
  | "START_FROM_TRIGGER"
  /** The node from which the trigger continues. Used to resume to the execution position of the master workflow when the execution of the child workflow is complete. No special configuration, similar to the END node. */
  | "CONTINUE_FROM_TRIGGER";

/**
 * Node status (advanced functionality, used for auditing, does not assume workflow execution logic)
 */
export type NodeStatus =
  /** Waiting for implementation */
  | "PENDING"
  /** under implementation */
  | "RUNNING"
  /** Implementation completed */
  | "COMPLETED"
  /** failure of execution */
  | "FAILED"
  /** Skipped (execution is marked by the graph algorithm and is an optional advanced feature) */
  | "SKIPPED"
  /** Cancelled */
  | "CANCELLED";

// ============================================================================
// Base Node Properties
// ============================================================================

/**
 * Base node attributes (common to all nodes)
 */
interface BaseNodeProps {
  /** Node Unique Identifier */
  id: ID;
  /** Node Name */
  name: string;
  /** Optional node description */
  description?: string;
  /** Optional metadata */
  metadata?: Metadata;
  /** Array of outgoing edge IDs for routing decisions */
  outgoingEdgeIds: ID[];
  /** Array of entry edge IDs for backtracking */
  incomingEdgeIds: ID[];
  /** Optional Dynamic Attribute Object */
  properties?: unknown[];
  /** Optional Hook Configuration Array */
  hooks?: unknown[];
  /** Whether checkpoints are created before node execution */
  checkpointBeforeExecute?: boolean;
  /** Whether checkpoints are created after node execution */
  checkpointAfterExecute?: boolean;
}

// ============================================================================
// Mapping of node types to configuration types
// ============================================================================

/**
 * Interface for mapping node types to configuration types
 * For type derivation and automatic generation of node types
 */
export interface NodeConfigMap {
  START: StartNodeConfig;
  END: EndNodeConfig;
  VARIABLE: VariableNodeConfig;
  FORK: ForkNodeConfig;
  JOIN: JoinNodeConfig;
  SUBGRAPH: SubgraphNodeConfig;
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
}

/**
 * Get the corresponding specific node type based on the node type
 * Auxiliary types for type derivation
 */
export type NodeOfType<T extends NodeType> = BaseNodeProps & {
  type: T;
  config: NodeConfigMap[T];
};

// ============================================================================
// Specific node type definitions (automatically generated using mapping types)
// ============================================================================

/**
 * start node
 */
export type StartNode = NodeOfType<"START">;

/**
 * end node
 */
export type EndNode = NodeOfType<"END">;

/**
 * variable node
 */
export type VariableNode = NodeOfType<"VARIABLE">;

/**
 * forked node
 */
export type ForkNode = NodeOfType<"FORK">;

/**
 * connection node
 */
export type JoinNode = NodeOfType<"JOIN">;

/**
 * subgraph node
 */
export type SubgraphNode = NodeOfType<"SUBGRAPH">;

/**
 * script node
 */
export type ScriptNode = NodeOfType<"SCRIPT">;

/**
 * LLM node
 */
export type LLMNode = NodeOfType<"LLM">;

/**
 * Tool to add nodes
 */
export type AddToolNode = NodeOfType<"ADD_TOOL">;

/**
 * user interaction node
 */
export type UserInteractionNode = NodeOfType<"USER_INTERACTION">;

/**
 * routing node
 */
export type RouteNode = NodeOfType<"ROUTE">;

/**
 * contextual processor node
 */
export type ContextProcessorNode = NodeOfType<"CONTEXT_PROCESSOR">;

/**
 * loop start node
 */
export type LoopStartNode = NodeOfType<"LOOP_START">;

/**
 * loop end node
 */
export type LoopEndNode = NodeOfType<"LOOP_END">;

/**
 * Agent Loop node
 */
export type AgentLoopNode = NodeOfType<"AGENT_LOOP">;

/**
 * Start node from trigger
 */
export type StartFromTriggerNode = NodeOfType<"START_FROM_TRIGGER">;

/**
 * Continue node from trigger
 */
export type ContinueFromTriggerNode = NodeOfType<"CONTINUE_FROM_TRIGGER">;

// ============================================================================
// Node association type
// ============================================================================

/**
 * Node union types (recognizable unions)
 * TypeScript automatically narrows the config type based on the type field.
 */
export type Node =
  | StartNode
  | EndNode
  | VariableNode
  | ForkNode
  | JoinNode
  | SubgraphNode
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

/**
 * 通用类型守卫工厂函数
 * 用于创建特定节点类型的类型守卫
 *
 * @example
 * const isStartNode = createNodeTypeGuard('START');
 * if (isStartNode(node)) {
 *   // node 自动收窄为 StartNode
 * }
 */
export function createNodeTypeGuard<T extends NodeType>(type: T) {
  return (node: unknown): node is NodeOfType<T> =>
    typeof node === "object" && node !== null && (node as Node).type === type;
}

/**
 * 检查节点是否为指定类型
 * 通用类型守卫函数，适用于所有节点类型
 *
 * @example
 * if (isNodeType(node, 'START')) {
 *   // node 自动收窄为 StartNode
 * }
 */
export function isNodeType<T extends NodeType>(node: unknown, type: T): node is NodeOfType<T> {
  return typeof node === "object" && node !== null && (node as Node).type === type;
}

// ============================================================================
// Type-specific guard functions (created using factory functions to maintain backward compatibility)
// ============================================================================

/**
 * Check if the node is of type START
 */
export const isStartNode = createNodeTypeGuard("START");

/**
 * Check if the node is of type END
 */
export const isEndNode = createNodeTypeGuard("END");

/**
 * Check if the node is of type VARIABLE
 */
export const isVariableNode = createNodeTypeGuard("VARIABLE");

/**
 * Check if the node is of type FORK
 */
export const isForkNode = createNodeTypeGuard("FORK");

/**
 * Check if the node is of type JOIN
 */
export const isJoinNode = createNodeTypeGuard("JOIN");

/**
 * Check if the node is of type SUBGRAPH
 */
export const isSubgraphNode = createNodeTypeGuard("SUBGRAPH");

/**
 * Check if the node is of type SCRIPT
 */
export const isScriptNode = createNodeTypeGuard("SCRIPT");

/**
 * Check if the node is of type LLM
 */
export const isLLMNode = createNodeTypeGuard("LLM");

/**
 * Check if the node is of type ADD_TOOL
 */
export const isAddToolNode = createNodeTypeGuard("ADD_TOOL");

/**
 * Check if the node is of type USER_INTERACTION
 */
export const isUserInteractionNode = createNodeTypeGuard("USER_INTERACTION");

/**
 * Check if the node is of type ROUTE
 */
export const isRouteNode = createNodeTypeGuard("ROUTE");

/**
 * Check if the node is of type CONTEXT_PROCESSOR
 */
export const isContextProcessorNode = createNodeTypeGuard("CONTEXT_PROCESSOR");

/**
 * Check if the node is of type LOOP_START
 */
export const isLoopStartNode = createNodeTypeGuard("LOOP_START");

/**
 * Check if the node is of type LOOP_END
 */
export const isLoopEndNode = createNodeTypeGuard("LOOP_END");

/**
 * Check if the node is of type AGENT_LOOP
 */
export const isAgentLoopNode = createNodeTypeGuard("AGENT_LOOP");

/**
 * Check if the node is of type START_FROM_TRIGGER
 */
export const isStartFromTriggerNode = createNodeTypeGuard("START_FROM_TRIGGER");

/**
 * Check if the node is of type CONTINUE_FROM_TRIGGER
 */
export const isContinueFromTriggerNode = createNodeTypeGuard("CONTINUE_FROM_TRIGGER");
