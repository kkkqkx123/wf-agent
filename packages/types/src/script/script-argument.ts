/**
 * Script Argument Type Definitions
 * Declares typed parameters for script templates with validation and default values
 */

/**
 * Script argument data type
 */
export type ScriptArgumentType = "string" | "number" | "boolean" | "file";

/**
 * Source of the argument value at runtime
 */
export type ArgumentValueSource = "static" | "variable" | "expression";

/**
 * Script Argument Declaration
 * Defines a typed parameter that can be injected into a script template
 */
export interface ScriptArgument {
  /** Argument key/name used in template placeholders (e.g., {{input.name}}) */
  key: string;
  /** Data type for validation */
  type: ScriptArgumentType;
  /** Human-readable label */
  label?: string;
  /** Human-readable description */
  description?: string;
  /** Default value if not provided */
  default?: unknown;
  /** Whether this argument is required */
  required?: boolean;
  /** Source of the value at runtime */
  source?: ArgumentValueSource;
  /** Allowed options (for enum-like selection) */
  options?: unknown[];
  /** Regex pattern for string validation */
  pattern?: string;
}