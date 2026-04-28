/**
 * Node Template Type Definition
 * Used to define reusable node configuration templates
 */

import type { ID, Metadata, Timestamp } from "./common.js";
import { NodeType } from "./node/index.js";
import type { NodeConfig } from "./node/index.js";

/**
 * Node Templates
 * Predefined node configuration templates that can be referenced by name in a workflow
 */
export interface NodeTemplate {
  /** Node template name (unique identifier) */
  name: string;
  /** Node type */
  type: NodeType;
  /** Node Configuration */
  config: NodeConfig;
  /** Node Description */
  description?: string;
  /** metadata */
  metadata?: Metadata;
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
}

/**
 * Node Reference Configuration
 * Used to reference predefined node templates in workflows
 */
export interface NodeReferenceConfig {
  /** The name of the referenced node template */
  templateName: string;
  /** Node ID (unique in the workflow) */
  nodeId: ID;
  /** Node name (displayed in the workflow, optional) */
  nodeName?: string;
  /** Configuration override (optional) */
  configOverride?: Partial<NodeConfig>;
}

/**
 * Node Template Summary Information
 */
export interface NodeTemplateSummary {
  /** Node template name */
  name: string;
  /** Node type */
  type: NodeType;
  /** Node Description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
}
