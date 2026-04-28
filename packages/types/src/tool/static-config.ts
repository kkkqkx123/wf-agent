/**
 * Tool Static Configuration Type Definition
 * (JSON Schema format for parameter definitions)
 */

import type { Metadata } from "../common.js";
import type { ToolRiskLevel } from "./risk-level.js";

/**
 * Tool parameter attribute types (based on JSON Schema Draft 2020-12)
 *
 * Note: Validation is handled by the LLM side, this type is only used for formatting and documentation purposes.
 */
export interface ToolProperty {
  /** types of counters (string, number, integer, boolean, array, object, null) */
  type: "string" | "number" | "integer" | "boolean" | "array" | "object" | "null";
  /** Parameter Description */
  description?: string;
  /** Default (optional) */
  default?: unknown;
  /** Enumerated values (optional) */
  enum?: unknown[];
  /** Format constraints (optional, such as uri, email, date, date-time, etc.) */
  format?: string;
  /** Example values (optional) */
  examples?: unknown[];

  // string constraint
  /** minimum length */
  minLength?: number;
  /** Maximum length */
  maxLength?: number;
  /** regular pattern */
  pattern?: string;

  // numerical constraint
  /** minimum value */
  minimum?: number;
  /** maximum values */
  maximum?: number;

  // object structure
  /** Object Property Definitions */
  properties?: Record<string, ToolProperty>;
  /** Required Attributes List */
  required?: string[];
  /** Additional Attribute Definitions */
  additionalProperties?: boolean | ToolProperty;

  // array structure
  /** Array Element Type Definition */
  items?: ToolProperty;

  // Allow other JSON Schema fields (for extensions)
  [key: string]: unknown;
}

/**
 * Tool parameter schema type (JSON Schema format)
 * This is the parameter definition structure used for LLM tool invocation.
 *
 * Note: The 'type: "object"' is fixed because tools always receive an object of parameters.
 * This follows OpenAI's tool calling convention.
 */
export interface ToolParameterSchema {
  /** Type (fixed to object for tool parameters) */
  type: "object";
  /** Parameter Attribute Definition */
  properties: Record<string, ToolProperty>;
  /** Required Parameters List */
  required: string[];
  /** Additional Attribute Definitions (must be false in strict mode) */
  additionalProperties?: boolean | ToolProperty;
}

/**
 * Approval condition type
 */
export type ApprovalConditionType =
  | "workspace_boundary"
  | "protected_file"
  | "command_whitelist"
  | "domain_whitelist"
  | "mcp_server_whitelist";

/**
 * Approval condition for fine-grained control
 */
export interface ApprovalCondition {
  /** Condition type */
  type: ApprovalConditionType;
  /** Condition configuration */
  config: Record<string, unknown>;
}

/**
 * Tool metadata types
 */
export interface ToolMetadata {
  /** Tool Classification */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Document URL (optional) */
  documentationUrl?: string;
  /** Custom Fields */
  customFields?: Metadata;

  // === Auto-approval fields ===
  /** Risk level for auto-approval decision */
  riskLevel?: ToolRiskLevel;
  /** Whether this tool can be auto-approved (overrides risk level check) */
  autoApprovable?: boolean;
  /** Approval conditions that must be met for auto-approval */
  approvalConditions?: ApprovalCondition[];
}