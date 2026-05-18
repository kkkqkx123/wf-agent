/**
 * Variable Node Configuration Type Definition
 */

/**
 * Variable Operation Node Configuration
 */
export interface VariableNodeConfig {
  /** The name of the variable to be operated on */
  variableName: string;
  /** The type of variable to operate on [contains number, string, boolean, array, object]. */
  variableType: 'number' | 'string' | 'boolean' | 'array' | 'object';
  /** Expression for the operation [directly overriding the corresponding variable with an expression] */
  expression: string;
  /** Read-only or not */
  readonly?: boolean;
}