/**
 * Type of workflow definition
 */

import type { Node } from "../node/index.js";
import type { Edge } from "../edge.js";
import type { ID, Version, Timestamp } from "../common.js";
import type { WorkflowTrigger } from "../trigger/index.js";
import type { TriggerReference } from "../trigger-template.js";
import type { WorkflowType } from "./type.js";
import type { WorkflowVariable } from "./variables.js";
import type { WorkflowConfig } from "./config.js";
import type { WorkflowMetadata } from "./metadata.js";
import type { TriggeredSubworkflowConfig } from "./config.js";

/**
 * Workflow Definition Type
 * Contains the basic information and structure of the workflow
 */
export interface WorkflowDefinition {
  /** Workflow unique identifier */
  id: ID;
  /** Workflow name */
  name: string;
  /** Type of workflow */
  type: WorkflowType;
  /** Optional workflow description */
  description?: string;
  /** Array of nodes, defining all nodes of the workflow */
  nodes: Node[];
  /** Array of edges, defining the connections between nodes */
  edges: Edge[];
  /** Array of workflow variable definitions for declaring variables required for workflow execution */
  variables?: WorkflowVariable[];
  /** Workflow Trigger Definitions array for declaring workflow level triggers */
  triggers?: (WorkflowTrigger | TriggerReference)[];
  /** Trigger subworkflow-specific configuration (only for workflows containing START_FROM_TRIGGER nodes) */
  triggeredSubworkflowConfig?: TriggeredSubworkflowConfig;
  /** Optional workflow configuration */
  config?: WorkflowConfig;
  /** Optional metadata information */
  metadata?: WorkflowMetadata;
  /** Workflow version number */
  version: Version;
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
  /** Available tool configurations */
  availableTools?: {
    /** Initial set of available tools (tool ID or name) */
    initial: Set<string>;
  };
}
