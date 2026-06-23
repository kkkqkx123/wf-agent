/**
 * Variable Node Output
 * - variableName: string - The name of the variable that was modified
 * - oldValue?: unknown - The previous value before modification
 * - newValue: unknown - The new value after modification
 */
export interface VariableNodeOutput {
  variableName: string;
  oldValue?: unknown;
  newValue: unknown;
}

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