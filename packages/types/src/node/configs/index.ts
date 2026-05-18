/**
 * Unified export of node configuration types
 */

// Exporting Control Node Configurations
export * from './control-configs.js';

// Exporting Variable Node Configurations
export * from './variable-configs.js';

// Export fork/merge node configurations
export * from './fork-join-configs.js';

// Exporting cyclic node configurations
export * from './loop-configs.js';

// Exporting Execution Node Configurations
export * from './execution-configs.js';

// Exporting Interactive Node Configurations
export * from './interaction-configs.js';

// Exporting Context Node Configurations
export * from './context-configs.js';

// Export subgraph node configuration
export * from './subgraph-configs.js';

// Export embed graph node configuration
export * from './embed-graph-configs.js';

// Export trigger-based subworkflow node configurations
// Note: These are aliases to WorkflowStartConfig and WorkflowEndConfig
export {
  WorkflowStartConfig as StartFromTriggerNodeConfig,
  WorkflowEndConfig as ContinueFromTriggerNodeConfig,
} from '../../workflow/boundary-config.js';

// Export agent loop node configuration
export * from './agent-loop-configs.js';

// Export sync node configuration
export * from './sync-configs.js';

// Export Zod Schemas for Node Configurations
export {
  ContextProcessorNodeConfigSchema,
  isContextProcessorNodeConfig,
} from './context-configs-schema.js';

export {
  SubgraphNodeConfigSchema,
  isSubgraphNodeConfig,
} from './subgraph-configs-schema.js';

export {
  EmbedGraphNodeConfigSchema,
} from './embed-graph-configs-schema.js';

// Re-export boundary config schemas for trigger nodes (aliases)
export {
  WorkflowStartConfigSchema as StartFromTriggerNodeConfigSchema,
  WorkflowEndConfigSchema as ContinueFromTriggerNodeConfigSchema,
  isWorkflowStartConfig as isStartFromTriggerNodeConfig,
  isWorkflowEndConfig as isContinueFromTriggerNodeConfig,
} from '../../workflow/boundary-config-schema.js';

export {
  LLMNodeConfigSchema,
  ScriptNodeConfigSchema,
  AddToolNodeConfigSchema,
  isLLMNodeConfig,
  isScriptNodeConfig,
  isAddToolNodeConfig,
} from './execution-configs-schema.js';

export {
  ForkNodeConfigSchema,
  JoinNodeConfigSchema,
  isForkNodeConfig,
  isJoinNodeConfig,
} from './fork-join-configs-schema.js';

export {
  LoopStartNodeConfigSchema,
  LoopEndNodeConfigSchema,
  isLoopStartNodeConfig,
  isLoopEndNodeConfig,
} from './loop-configs-schema.js';

export {
  VariableNodeConfigSchema,
  isVariableNodeConfig,
} from './variable-configs-schema.js';

export {
  RouteNodeConfigSchema,
  isRouteNodeConfig,
} from './control-configs-schema.js';

export {
  UserInteractionNodeConfigSchema,
  isUserInteractionNodeConfig,
} from './interaction-configs-schema.js';

export {
  AgentLoopNodeConfigSchema,
  isAgentLoopNodeConfig,
} from './agent-loop-configs-schema.js';

export {
  SyncNodeConfigSchema,
  isSyncNodeConfig,
} from './sync-configs-schema.js';