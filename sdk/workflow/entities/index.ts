/**
 * Workflow Entities Module
 *
 * This module exports all entity classes for the workflow execution system.
 */

export { WorkflowGraphData } from "./workflow-graph-data.js";
export { WorkflowGraphEntity } from "./workflow-graph.js";
export { WorkflowExecutionEntity } from "./workflow-execution-entity.js";

// Backward compatibility aliases
export { WorkflowGraphData as GraphData } from "./workflow-graph-data.js";
export { WorkflowGraphEntity as PreprocessedGraphData } from "./workflow-graph.js";
