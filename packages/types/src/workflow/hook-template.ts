/**
 * HookTemplate Type Definition
 * Used to define reusable hook configuration templates.
 *
 * Design principles:
 * - Follows the same pattern as NodeTemplate and TriggerTemplate
 * - Enables reuse of hook configurations across nodes and workflows
 * - Provides standardized metadata and lifecycle fields
 */

import type { Metadata, Timestamp } from "../common.js";
import type { NodeHook } from "../node/hooks.js";

/**
 * HookTemplate - Reusable hook configuration template.
 *
 * Allows defining a hook configuration once and referencing it
 * by name across multiple nodes or workflows.
 */
export interface HookTemplate {
  /** Hook template name (unique identifier) */
  name: string;
  /** Hook configuration */
  hook: NodeHook;
  /** Template description */
  description?: string;
  /** Metadata (category, tags, custom fields) */
  metadata?: Metadata;
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * HookTemplateReference - Reference a HookTemplate by name in a node configuration.
 *
 * When a node references a hook template, optional overrides can be
 * provided to customize the hook behavior for that specific node.
 */
export interface HookTemplateReference {
  /** Name of the hook template to reference */
  templateName: string;
  /** Optional overrides to merge into the resolved hook */
  overrides?: {
    /** Override event payload */
    eventPayload?: Record<string, unknown>;
    /** Override enabled state */
    enabled?: boolean;
    /** Override priority weight */
    weight?: number;
    /** Override checkpoint settings */
    createCheckpoint?: boolean;
    /** Override checkpoint description */
    checkpointDescription?: string;
  };
}

/**
 * HookTemplateSummary - Lightweight summary for listing/display.
 */
export interface HookTemplateSummary {
  /** Hook template name */
  name: string;
  /** Hook type */
  hookType: string;
  /** Template description */
  description?: string;
  /** Category */
  category?: string;
  /** Tags */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}