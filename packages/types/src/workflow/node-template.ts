/**
 * Node Template Type Definition
 * Used to define reusable node configuration templates
 */

import type { Metadata, Timestamp } from "../common.js";
import { StaticNodeType } from "../node/index.js";
import type { StaticNode } from "../node/index.js";

/**
 * Node Templates
 * Predefined node configuration templates that can be referenced by name in a workflow
 */
export interface NodeTemplate {
  /** Node template name (unique identifier) */
  name: string;
  /** Node type */
  type: StaticNodeType;
  /** Node Configuration */
  config: StaticNode["config"];
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
 * Node Template Summary Information
 */
export interface NodeTemplateSummary {
  /** Node template name */
  name: string;
  /** Node type */
  type: StaticNodeType;
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
