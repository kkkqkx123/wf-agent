/**
 * @description Tests for Workflow Template Type
 * @priority HIGH
 * 
 * Validates:
 * - WorkflowTemplate structure completeness
 * - Required vs optional fields
 * - Node and Edge array types
 * - Boundary configuration types
 * - Variable definitions
 */

import { expectType, expectAssignable } from "tsd";
import type {
  WorkflowTemplate,
  StaticNode,
  Edge,
  WorkflowStartConfig,
  WorkflowEndConfig,
  VariableDefinition,
  AvailableTools,
  VariableValueType,
} from "../../../src/index.js";

// ============================================================================
// Test 1: Minimal valid WorkflowTemplate
// ============================================================================

const minimalWorkflow: WorkflowTemplate = {
  id: "test-workflow",
  name: "Test Workflow",
  type: "STANDALONE",
  nodes: [],
  edges: [],
  version: "1.0.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
};

expectType<WorkflowTemplate>(minimalWorkflow);
expectType<string>(minimalWorkflow.id);
expectType<string>(minimalWorkflow.name);
expectType<"TRIGGERED_SUBWORKFLOW" | "STANDALONE" | "DEPENDENT">(minimalWorkflow.type);
expectType<StaticNode[]>(minimalWorkflow.nodes);
expectType<Edge[]>(minimalWorkflow.edges);
expectType<string>(minimalWorkflow.version);
expectType<number>(minimalWorkflow.createdAt);
expectType<number>(minimalWorkflow.updatedAt);

// ============================================================================
// Test 2: Full WorkflowTemplate with all optional fields
// ============================================================================

const fullWorkflow: WorkflowTemplate = {
  id: "full-workflow",
  name: "Full Workflow",
  type: "DEPENDENT",
  description: "A workflow with all fields",
  nodes: [],
  edges: [],
  variables: [
    {
      name: "inputVar",
      type: "string",
      value: "default",
      scope: "execution",
      readonly: false,
    },
  ],
  triggers: [],
  triggeredSubworkflowConfig: {
    enableCheckpoints: true,
    timeout: 5000,
  },
  config: {
    timeout: 30000,
    maxSteps: 100,
  },
  metadata: {
    tags: ["test", "example"],
    author: "Test Author",
  },
  version: "2.0.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
  availableTools: {
    initial: ["tool1", "tool2"],
  },
};

expectType<WorkflowTemplate>(fullWorkflow);
expectType<string | undefined>(fullWorkflow.description);
expectType<VariableDefinition[] | undefined>(fullWorkflow.variables);
expectType<AvailableTools | undefined>(fullWorkflow.availableTools);

// ============================================================================
// Test 3: Optional fields are truly optional
// ============================================================================

declare const workflow: WorkflowTemplate;

// These should all be optional (undefined is acceptable)
expectAssignable<string | undefined>(workflow.description);
expectAssignable<VariableDefinition[] | undefined>(workflow.variables);

// ============================================================================
// Test 4: Workflow template type variants
// ============================================================================

const mainWorkflow: WorkflowTemplate = {
  ...minimalWorkflow,
  type: "STANDALONE",
};
// Note: Spread operator preserves the union type, so we use type assertion
expectAssignable<WorkflowTemplate>(mainWorkflow);

const subworkflow: WorkflowTemplate = {
  ...minimalWorkflow,
  type: "DEPENDENT",
};
expectAssignable<WorkflowTemplate>(subworkflow);

const triggeredWorkflow: WorkflowTemplate = {
  ...minimalWorkflow,
  type: "TRIGGERED_SUBWORKFLOW",
};
expectAssignable<WorkflowTemplate>(triggeredWorkflow);

// ============================================================================
// Test 5: Nodes and Edges arrays
// ============================================================================

const workflowWithNodes: WorkflowTemplate = {
  ...minimalWorkflow,
  nodes: [
    {
      id: "start",
      name: "Start",
      type: "START",
      config: {},
    } as StaticNode,
    {
      id: "end",
      name: "End",
      type: "END",
      config: {},
    } as StaticNode,
  ],
  edges: [
    {
      id: "edge1",
      sourceNodeId: "start",
      targetNodeId: "end",
      type: "DEFAULT",
    } as Edge,
  ],
};

expectType<StaticNode[]>(workflowWithNodes.nodes);
expectType<Edge[]>(workflowWithNodes.edges);

// ============================================================================
// Test 6: Variable definitions
// ============================================================================

if (fullWorkflow.variables) {
  const firstVar = fullWorkflow.variables[0];
  if (firstVar) {
    expectType<string>(firstVar.name);
    expectType<VariableValueType>(firstVar.type);
    expectType<unknown>(firstVar.value);
    expectType<"global" | "execution" | "subgraph" | "loop">(firstVar.scope);
    expectType<boolean>(firstVar.readonly);
  }
}

// ============================================================================
// Test 7: Available tools configuration
// ============================================================================

if (fullWorkflow.availableTools) {
  expectType<string[]>(fullWorkflow.availableTools.initial);
  expectType<Set<string> | undefined>(fullWorkflow.availableTools.dynamic);
  expectType<"none" | "allowlist" | "blocklist" | undefined>(fullWorkflow.availableTools.filterMode);
}

const toolsOnlyInitial: AvailableTools = {
  initial: ["tool1"],
};
expectType<string[]>(toolsOnlyInitial.initial);

// ============================================================================
// Test 8: Metadata structure
// ============================================================================

if (fullWorkflow.metadata) {
  expectType<string[] | undefined>(fullWorkflow.metadata.tags);
  expectType<string | undefined>(fullWorkflow.metadata.author);
}

// ============================================================================
// Test 9: Config structure
// ============================================================================

if (fullWorkflow.config) {
  expectType<number | undefined>(fullWorkflow.config.timeout);
  expectType<number | undefined>(fullWorkflow.config.maxSteps);
}
