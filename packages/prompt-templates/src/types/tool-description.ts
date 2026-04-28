/**
 * Tool Description Data Type Definitions
 *
 * Defines the data structure of the tool description for use by the SDK side of the implementation.
 * This package only provides type definitions and rendering templates, it does not contain specific tool descriptions.
 */

/**
 * Description of tool parameters
 */
export interface ToolParameterDescription {
  /** Parameter name */
  name: string;
  /** Parameter type */
  type: string;
  /** Required or not */
  required: boolean;
  /** Parameter Description */
  description: string;
  /** default value */
  defaultValue?: unknown;
}

/**
 * Tool Description Data
 * SDK-side tool implementations should provide data constants for this structure
 */
export interface ToolDescriptionData {
  /** Tool name */
  name: string;
  /** Tool ID */
  id: string;
  /** Tool type */
  type: "STATELESS" | "STATEFUL";
  /** Tool Classification */
  category?: "filesystem" | "shell" | "memory" | "code" | "http";
  /** Tool Description */
  description: string;
  /** parameter list */
  parameters: ToolParameterDescription[];
  /** Tips for use */
  tips?: string[];
  /** usage example */
  examples?: string[];
}
