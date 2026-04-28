/**
 * Template Type Definition
 * Defines the core data structure for the suggestion template
 */

/**
 * Variable Definitions
 * Describes the variables used in the template, as well as their types and constraints.
 */
export interface VariableDefinition {
  /** Variable name */
  name: string;
  /** Variable Types */
  type: "string" | "number" | "boolean" | "object" | "array";
  /** Is it mandatory? */
  required: boolean;
  /** Variable Description */
  description?: string;
  /** Default value */
  defaultValue?: unknown;
}

/**
 * Prompt Template
 * Define a complete prompt template structure
 */
export interface PromptTemplate {
  /** Template Unique Identifier */
  id: string;
  /** Template Name */
  name: string;
  /** Template description */
  description: string;
  /** Template Category */
  category: "system" | "rules" | "user-command" | "tools" | "composite" | "fragments" | "dynamic";
  /** Template content, including the {{variable}} placeholder */
  content: string;
  /** Required variable definitions */
  variables?: VariableDefinition[];
  /** List of referenced fragment IDs */
  fragments?: string[];
}

/**
 * Template Filling Rules
 * Define how to fill context data into templates.
 */
export interface TemplateFillRule {
  /** Template ID */
  templateId: string;
  /** Mapping of variable names to context paths */
  variableMapping: Record<string, string>;
  /** Mapping of Fragment IDs to Actual Content */
  fragmentMapping?: Record<string, string>;
}
