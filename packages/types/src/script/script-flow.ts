/**
 * Script Flow Blueprint Type Definitions
 * Defines multi-step, multi-branch script execution flows
 */

/**
 * Reference to a module within a flow branch
 * Points to a registered Script and optionally overrides its arguments
 */
export interface FlowModuleRef {
  /** Script key/name referencing a registered Script */
  key: string;
  /** Optional argument overrides for this module invocation */
  args?: Record<string, unknown>;
}

/**
 * A single branch in a script flow
 * Contains an ordered list of modules to execute sequentially
 */
export interface FlowBranch {
  /** Unique branch identifier */
  key: string;
  /** Ordered list of module references to execute */
  modules: FlowModuleRef[];
  /** Branches that this branch depends on (for topological ordering) */
  depends_on?: string[];
}

/**
 * Script Flow Blueprint
 * Defines a multi-branch, multi-step script execution plan
 */
export interface ScriptFlow {
  /** Flow name */
  name: string;
  /** Flow description */
  description?: string;
  /** Branches to execute (parallel if no dependencies, sequential otherwise) */
  branches: FlowBranch[];
}