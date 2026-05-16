/**
 * Node Type Definition Unified Export
 * 
 * Architecture:
 * - StaticNode types: Used for workflow definition, validation, preprocessing
 * - RuntimeNode types: Used for execution after preprocessing
 * - SUBGRAPH nodes create independent execution entities at runtime (Phase 1: Scheme C)
 * - EMBED_GRAPH nodes are expanded during preprocessing for lightweight reuse (Phase 3)
 */

// Export shared base types
export {
  NodeIdentity,
  StaticNodeDisplayProps,
  NodeExecutionConfig,
  RuntimeNodeContext,
} from "./shared-node-types.js";

// Export static node types (for workflow definition and validation)
export {
  StaticNodeType,
  BaseStaticNode,
  StaticNodeConfigMap,
  StaticNodeOfType,
  // Specific static node types
  StartNode as StaticStartNode,
  EndNode as StaticEndNode,
  VariableNode as StaticVariableNode,
  ForkNode as StaticForkNode,
  JoinNode as StaticJoinNode,
  SubgraphNode,
  EmbedGraphNode,
  ScriptNode as StaticScriptNode,
  LLMNode as StaticLLMNode,
  AddToolNode as StaticAddToolNode,
  UserInteractionNode as StaticUserInteractionNode,
  RouteNode as StaticRouteNode,
  ContextProcessorNode as StaticContextProcessorNode,
  LoopStartNode as StaticLoopStartNode,
  LoopEndNode as StaticLoopEndNode,
  AgentLoopNode as StaticAgentLoopNode,
  StartFromTriggerNode as StaticStartFromTriggerNode,
  ContinueFromTriggerNode as StaticContinueFromTriggerNode,
  StaticNode,
  // Static type guards
  isStartNode as isStaticStartNode,
  isEndNode as isStaticEndNode,
  isVariableNode as isStaticVariableNode,
  isForkNode as isStaticForkNode,
  isJoinNode as isStaticJoinNode,
  isSubgraphNode,
  isEmbedGraphNode,
  isScriptNode as isStaticScriptNode,
  isLLMNode as isStaticLLMNode,
  isAddToolNode as isStaticAddToolNode,
  isUserInteractionNode as isStaticUserInteractionNode,
  isRouteNode as isStaticRouteNode,
  isContextProcessorNode as isStaticContextProcessorNode,
  isLoopStartNode as isStaticLoopStartNode,
  isLoopEndNode as isStaticLoopEndNode,
  isAgentLoopNode as isStaticAgentLoopNode,
  isStartFromTriggerNode as isStaticStartFromTriggerNode,
  isContinueFromTriggerNode as isStaticContinueFromTriggerNode,
} from "./static-node-types.js";

// Export runtime node types (for execution)
export {
  RuntimeNodeType,
  BaseRuntimeNode,
  RuntimeNodeConfigMap,
  RuntimeNodeOfType,
  // Specific runtime node types
  StartNode as RuntimeStartNode,
  EndNode as RuntimeEndNode,
  VariableNode as RuntimeVariableNode,
  ForkNode as RuntimeForkNode,
  JoinNode as RuntimeJoinNode,
  SubgraphNode as RuntimeSubgraphNode,  // Exists at runtime (Phase 1: Scheme C)
  ScriptNode as RuntimeScriptNode,
  LLMNode as RuntimeLLMNode,
  AddToolNode as RuntimeAddToolNode,
  UserInteractionNode as RuntimeUserInteractionNode,
  RouteNode as RuntimeRouteNode,
  ContextProcessorNode as RuntimeContextProcessorNode,
  LoopStartNode as RuntimeLoopStartNode,
  LoopEndNode as RuntimeLoopEndNode,
  AgentLoopNode as RuntimeAgentLoopNode,
  StartFromTriggerNode as RuntimeStartFromTriggerNode,
  ContinueFromTriggerNode as RuntimeContinueFromTriggerNode,
  // Internal types (used ONLY for EMBED_GRAPH expansion, not for public use)
  EmbedStartNode,
  EmbedEndNode,
  RuntimeNode,
  // Runtime type guards
  isStartNode as isRuntimeStartNode,
  isEndNode as isRuntimeEndNode,
  isVariableNode as isRuntimeVariableNode,
  isForkNode as isRuntimeForkNode,
  isJoinNode as isRuntimeJoinNode,
  isSubgraphNode as isRuntimeSubgraphNode,  // Exists at runtime
  isScriptNode as isRuntimeScriptNode,
  isLLMNode as isRuntimeLLMNode,
  isAddToolNode as isRuntimeAddToolNode,
  isUserInteractionNode as isRuntimeUserInteractionNode,
  isRouteNode as isRuntimeRouteNode,
  isContextProcessorNode as isRuntimeContextProcessorNode,
  isLoopStartNode as isRuntimeLoopStartNode,
  isLoopEndNode as isRuntimeLoopEndNode,
  isAgentLoopNode as isRuntimeAgentLoopNode,
  isStartFromTriggerNode as isRuntimeStartFromTriggerNode,
  isContinueFromTriggerNode as isRuntimeContinueFromTriggerNode,
  // Internal type guards (used ONLY for EMBED_GRAPH expansion, not for public use)
  isEmbedStartNode,
  isEmbedEndNode,
} from "./runtime-node-types.js";



// Export node configuration types (detailed version for external references)
export * from "./configs/index.js";

// Export Hook Related Types
export * from "./hooks.js";
