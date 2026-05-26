/**
 * Type of workflow definition
 */

import type { StaticNode } from "../node/index.js";
import type { Edge } from "./edge.js";
import type { ID, Version, Timestamp } from "../common.js";
import type { WorkflowTrigger } from "../trigger/index.js";
import type { TriggerReference } from "../trigger/template.js";
import type { VariableDefinition } from "../workflow-execution/variables.js";
import type { WorkflowConfig } from "./config.js";
import type { TriggeredSubworkflowConfig } from "./config.js";
import type { AvailableTools } from "./tool-config.js";

/**
 * Workflow Template Types
 * Used to differentiate between different types of workflows, affecting preprocessing timing and checkpointing strategies
 */
export type WorkflowTemplateType =
  /** Trigger subworkflow: must contain START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER nodes, not start, end, subgraph nodes */
  | "TRIGGERED_SUBWORKFLOW"
  /** Standalone workflow: does not contain EXECUTE_TRIGGERED_SUBGRAPH triggers and does not contain SUBGRAPH nodes */
  | "STANDALONE"
  /** Dependent workflow: contains EXECUTE_TRIGGERED_SUBGRAPH trigger or SUBGRAPH node */
  | "DEPENDENT";

/**
 * Workflow Metadata Types
 * Used to store extended information
 */
export interface WorkflowMetadata {
  /** Author Information */
  author?: string;
  /** tagged array */
  tags?: string[];
  /** categorization */
  category?: string;
}

/**
 * Workflow Template Type
 *
 * Contains the basic information and structure of a workflow definition.
 * This type is used for both file-based configuration (TOML/JSON) and runtime execution.
 *
 * Key characteristics:
 * - Pure data structure with no executable functions (serializable to JSON/TOML)
 * - Includes metadata, versioning, and structural elements (nodes, edges)
 * - Used by SDK's config parser to load workflows from files
 * - Used directly by workflow engine during execution
 */
export interface WorkflowTemplate {
  /** Workflow unique identifier (required) */
  id: ID;

  /** Workflow name (required) */
  name: string;

  /** Type of workflow (e.g., 'main', 'subworkflow', 'triggered') */
  type: WorkflowTemplateType;

  /** Optional workflow description for documentation purposes */
  description?: string;

  /** Array of nodes defining all execution steps in the workflow */
  nodes: StaticNode[];

  /** Array of edges defining the connections and flow between nodes */
  edges: Edge[];

  /** Array of workflow variable definitions for declaring variables required during execution */
  variables?: VariableDefinition[];

  /** Workflow trigger definitions for declaring workflow-level event triggers */
  triggers?: (WorkflowTrigger | TriggerReference)[];

  /** Trigger subworkflow-specific configuration (only for workflows containing START_FROM_TRIGGER nodes) */
  triggeredSubworkflowConfig?: TriggeredSubworkflowConfig;

  /** Optional workflow execution behavior configuration (timeout, retry, etc.) */
  config?: WorkflowConfig;

  /** Optional metadata information (tags, author, custom properties) */
  metadata?: WorkflowMetadata;

  /** Workflow version number for tracking changes */
  version: Version;

  /** Creation timestamp (ISO 8601 format) */
  createdAt: Timestamp;

  /** Last update timestamp (ISO 8601 format) */
  updatedAt: Timestamp;

  /**
   * Available tool configurations for LLM nodes
   * 
   * Unified interface for specifying which tools are available during workflow execution.
   * Supports static initial tools and dynamic additions during execution.
   */
  availableTools?: AvailableTools;
}
