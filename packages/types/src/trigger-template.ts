/**
 * TriggerTemplate Type Definition
 * Defines the type and structure of a trigger template, which is used to enable reuse of trigger configurations.
 *
 * design principles:
 * - TriggerTemplate is specialized for configuration reuse
 * - Support reuse of trigger configurations across workflows
 * - Provide a standardized interface
 * - Easy serialization and deserialization
 */

import type { ID, Timestamp, Metadata } from "./common.js";
import type { TriggerCondition } from "./trigger/index.js";
import type { TriggerAction } from "./trigger/index.js";

/**
 * Trigger Template Definitions
 * Used to predefine trigger configurations to support referencing and reuse in workflows
 */
export interface TriggerTemplate {
  /** Trigger template name (unique identifier) */
  name: string;
  /** Trigger Description */
  description?: string;
  /** trigger condition */
  condition: TriggerCondition;
  /** trigger action */
  action: TriggerAction;
  /** Enable or not (default true) */
  enabled?: boolean;
  /** Limit on the number of triggers (0 means no limit) */
  maxTriggers?: number;
  /** metadata */
  metadata?: Metadata;
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
  /** Whether to create checkpoints when triggered (new) */
  createCheckpoint?: boolean;
  /** Checkpoint description template (new) */
  checkpointDescriptionTemplate?: string;
}

/**
 * Trigger References
 * Referencing predefined trigger templates in a workflow
 */
export interface TriggerReference {
  /** The name of the referenced trigger template */
  templateName: string;
  /** Trigger ID (unique in the workflow) */
  triggerId: ID;
  /** Trigger name (displayed in the workflow, optional) */
  triggerName?: string;
  /** Configuration override (optional) */
  configOverride?: TriggerConfigOverride;
}

/**
 * Trigger Configuration Override
 * Allows partial configuration override when referencing a template
 */
export interface TriggerConfigOverride {
  /** condition coverage */
  condition?: Partial<TriggerCondition>;
  /** Motion Override */
  action?: Partial<TriggerAction>;
  /** Whether to enable overrides */
  enabled?: boolean;
  /** Trigger count limit override */
  maxTriggers?: number;
}

/**
 * Trigger Template Summary
 * Used for list presentation, does not contain full configuration
 */
export interface TriggerTemplateSummary {
  /** Trigger Template Name */
  name: string;
  /** Trigger Description */
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

/**
 * Trigger Template Filter
 * For querying trigger templates
 */
export interface TriggerTemplateFilter {
  /** Trigger Template Name */
  name?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Search Keywords */
  keyword?: string;
}
