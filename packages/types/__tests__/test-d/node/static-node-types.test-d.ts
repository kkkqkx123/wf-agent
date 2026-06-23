/**
 * @description Tests for Static Node Type System
 * @priority HIGH
 * 
 * Validates:
 * - StaticNodeType discriminated union
 * - StaticNodeConfigMap generic mapping
 * - StaticNodeOfType type inference
 * - Type guards for static nodes
 */

import { expectType, expectAssignable } from "tsd";
import type {
  StaticNode,
  StaticNodeType,
  StaticNodeOfType,
  StaticLLMNode,
  StaticScriptNode,
  StaticForkNode,
  ScriptRiskLevel,
} from "../../../src/index.js";
import {
  isStaticLLMNode,
  isStaticScriptNode,
  isStaticForkNode,
} from "../../../src/index.js";

// ============================================================================
// Test 1: StaticNodeType is a valid union
// ============================================================================

declare const nodeType: StaticNodeType;
expectAssignable<"START" | "END" | "VARIABLE" | "FORK" | "JOIN" | "SUBGRAPH" | "SYNC" | "SCRIPT" | "LLM" | "TOOL_VISIBILITY" | "USER_INTERACTION" | "ROUTE" | "CONTEXT_PROCESSOR" | "LOOP_START" | "LOOP_END" | "AGENT_LOOP" | "START_FROM_TRIGGER" | "CONTINUE_FROM_TRIGGER" | "EMBED_GRAPH">(nodeType);

// ============================================================================
// Test 2: StaticNodeOfType generic type inference
// ============================================================================

// LLM node should have correct config type
type LLMPartialNode = StaticNodeOfType<"LLM">;
declare const llmNode: LLMPartialNode;
expectType<"LLM">(llmNode.type);
// Config should be LLMNodeConfig with profileId field
expectType<string>(llmNode.config.profileId);
expectType<string | undefined>(llmNode.config.contextId);

// Script node should have scriptName and risk fields
type ScriptPartialNode = StaticNodeOfType<"SCRIPT">;
declare const scriptNode: ScriptPartialNode;
expectType<"SCRIPT">(scriptNode.type);
expectType<string>(scriptNode.config.scriptName);
expectType<ScriptRiskLevel>(scriptNode.config.risk);

// Fork node should have forkPaths
type ForkPartialNode = StaticNodeOfType<"FORK">;
declare const forkNode: ForkPartialNode;
expectType<"FORK">(forkNode.type);
expectType<Array<{ pathId: string; childNodeId: string }>>(forkNode.config.forkPaths);

// ============================================================================
// Test 3: StaticNode union type structure
// ============================================================================

declare const anyStaticNode: StaticNode;
expectType<StaticNodeType>(anyStaticNode.type);
expectType<string>(anyStaticNode.id);
expectType<string>(anyStaticNode.name);

// ============================================================================
// Test 4: Type guards narrow types correctly
// ============================================================================

// Test isStaticLLMNode
declare const maybeLLMNode: StaticNode;
if (isStaticLLMNode(maybeLLMNode)) {
  expectType<StaticLLMNode>(maybeLLMNode);
  expectType<"LLM">(maybeLLMNode.type);
  expectType<string>(maybeLLMNode.config.profileId);
  expectType<string | undefined>(maybeLLMNode.config.contextId);
}

// Test isStaticScriptNode
declare const maybeScriptNode: StaticNode;
if (isStaticScriptNode(maybeScriptNode)) {
  expectType<StaticScriptNode>(maybeScriptNode);
  expectType<"SCRIPT">(maybeScriptNode.type);
  expectType<string>(maybeScriptNode.config.scriptName);
  expectType<ScriptRiskLevel>(maybeScriptNode.config.risk);
}

// Test isStaticForkNode
declare const maybeForkNode: StaticNode;
if (isStaticForkNode(maybeForkNode)) {
  expectType<StaticForkNode>(maybeForkNode);
  expectType<"FORK">(maybeForkNode.type);
  expectType<Array<{ pathId: string; childNodeId: string }>>(maybeForkNode.config.forkPaths);
}

// ============================================================================
// Test 5: Switch statement type narrowing
// ============================================================================

function processNodeByType(node: StaticNode): void {
  switch (node.type) {
    case "LLM":
      // Should narrow to StaticLLMNode
      expectType<StaticLLMNode>(node);
      expectType<string>(node.config.profileId);
      break;
    
    case "SCRIPT":
      // Should narrow to StaticScriptNode
      expectType<StaticScriptNode>(node);
      expectType<string>(node.config.scriptName);
      break;
    
    case "FORK":
      // Should narrow to StaticForkNode
      expectType<StaticForkNode>(node);
      expectType<Array<{ pathId: string; childNodeId: string }>>(node.config.forkPaths);
      break;
    
    case "START":
      // Should narrow to StaticStartNode
      expectType<string>(node.id);
      break;
  }
}

// ============================================================================
// Test 6: Invalid type assignments should fail
// ============================================================================

// A LLM node should not be assignable to a Script node variable
declare const llmNodeFull: StaticLLMNode;
// This should cause a type error if uncommented:
// expectType<StaticScriptNode>(llmNodeFull);

// ============================================================================
// Test 7: Optional fields handling
// ============================================================================

declare const nodeWithOptional: StaticNode;
// description should be optional
expectType<string | undefined>(nodeWithOptional.description);
// metadata should be optional
expectType<Record<string, unknown> | undefined>(nodeWithOptional.metadata);
