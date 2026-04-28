/**
 * Node Type Definition Unified Export
 * Define the type and structure of workflow nodes
 */

// Export base types (including recognizable union Node types and type guards)
export * from "./base.js";

// Export node configuration type (detailed version for external references)
export * from "./configs/index.js";

// Export Hook Related Types
export * from "./hooks.js";

// Export Node Attribute Types
export * from "./properties.js";

// Exporting Agent Loop Types
export * from "./agent-loop.js";

// Export all node configured union types (for scenarios such as NodeTemplate)
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
  SubgraphNodeConfig,
  StartFromTriggerNodeConfig,
  ContinueFromTriggerNodeConfig,
} from "./configs/subgraph-configs.js";

import type { AgentLoopNodeConfig } from "./agent-loop.js";

/**
 * Node Configuration Union Type
 * Used in scenarios where arbitrary node configurations need to be accepted (e.g. NodeTemplate)
 */
export type NodeConfig =
  | StartNodeConfig
  | EndNodeConfig
  | VariableNodeConfig
  | ForkNodeConfig
  | JoinNodeConfig
  | ScriptNodeConfig
  | LLMNodeConfig
  | AddToolNodeConfig
  | UserInteractionNodeConfig
  | RouteNodeConfig
  | ContextProcessorNodeConfig
  | LoopStartNodeConfig
  | LoopEndNodeConfig
  | AgentLoopNodeConfig
  | SubgraphNodeConfig
  | StartFromTriggerNodeConfig
  | ContinueFromTriggerNodeConfig;
