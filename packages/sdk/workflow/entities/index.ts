/**
 * Workflow Entities Module
 *
 * This module exports all entity classes for the workflow execution system.
 */

export { WorkflowGraphStructureImpl } from "./workflow-graph-structure.js";
export { WorkflowGraphMetadata } from "./workflow-graph-metadata.js";
export { WorkflowGraph } from "./workflow-graph.js";
export { WorkflowExecutionEntity } from "./workflow-execution-entity.js";

// Backward compatibility aliases
export { WorkflowGraphStructureImpl as GraphData } from "./workflow-graph-structure.js";
export { WorkflowGraph as PreprocessedGraphData } from "./workflow-graph.js";