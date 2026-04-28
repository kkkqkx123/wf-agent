/**
 * Node Attribute Type Definition
 */

/**
 * Node Dynamic Attribute Types
 */
export interface NodeProperty {
  /** property key */
  key: string;
  /** attribute value */
  value: unknown;
  /** Attribute Type */
  type: string;
  /** Required or not */
  required: boolean;
  /** Validation rules */
  validation?: unknown;
}
