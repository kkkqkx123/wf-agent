/**
 * Type of workflow definition
 */

import type { Node } from "../node/index.js";
import type { Edge } from "../edge.js";
import type { ID, Version, Timestamp } from "../common.js";
import type { WorkflowTrigger } from "../trigger/index.js";
import type { TriggerReference } from "../trigger-template.js";
import type { WorkflowTemplateType } from "./type.js";
import type { WorkflowVariable } from "./variables.js";
import type { WorkflowConfig } from "./config.js";
import type { WorkflowMetadata } from "./metadata.js";
import type { TriggeredSubworkflowConfig } from "./config.js";

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
  nodes: Node[];

  /** Array of edges defining the connections and flow between nodes */
  edges: Edge[];

  /** Array of workflow variable definitions for declaring variables required during execution */
  variables?: WorkflowVariable[];

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

  /** Available tool configurations for LLM nodes */
  availableTools?: {
    /** Initial set of available tools (tool IDs). Array format for JSON/TOML serialization compatibility */
    initial: string[];
  };
}
