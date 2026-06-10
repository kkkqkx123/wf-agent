/**
 * VariableManager - Simplified Variable State Manager
 * 
 * Design Philosophy:
 * - Flat variable storage: All variables in a single Map
 * - Explicit variable passing through importVariables/exportVariables
 * - No implicit scope inheritance - all cross-boundary transfers are explicit
 * - Deep clone on import/export to ensure complete isolation
 * 
 * Key Features:
 * - Simple structure: Single flat Map for all variables
 * - Explicit data flow: Variables must be explicitly mapped at boundaries
 * - Automatic deep cloning: Prevents accidental state pollution
 * - Runtime validation: Checks variable access against declared inputs
 */

import type { VariableDefinition, WorkflowVariableInput, WorkflowVariableOutput } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { StateManager } from "../../core/types/state-manager.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "variable-manager" });

/**
 * Helper function to infer variable type from value
 */
function inferVariableType(value: unknown): "number" | "string" | "boolean" | "array" | "object" {
  if (Array.isArray(value)) return "array";
  if (value === null) return "object";
  const t = typeof value;
  if (t === "number" || t === "string" || t === "boolean") return t;
  return "object";
}

/**
 * Variable Entry - Internal representation
 */
interface VariableEntry {
  definition: VariableDefinition;
  value: unknown;
}

/**
 * VariableManager Snapshot
 */
export interface VariableManagerSnapshot {
  variables: Map<string, VariableEntry>;
}

/**
 * VariableManager - Simplified Variable State Manager
 * 
 * Responsibilities:
 * - Manage runtime state of variables using a single Map
 * - Provide workflow-execution-isolated state management
 * - Support state snapshots and restoration
 * - Offer atomic state operations
 * 
 * ⚠️ IMPORTANT: Object Reference Sharing
 * Snapshots use shallow copy for variable values. Object values are shared by reference,
 * not deep copied. This means:
 * ```typescript
 * manager.setVariable("data", { count: 1 });
 * const snapshot = manager.createSnapshot();
 * manager.setVariable("data", { count: 2 }); // Creates new object, safe
 * 
 * // BUT if you modify the object directly:
 * const obj = manager.getVariable("config") as any;
 * obj.newProp = "value"; // This affects ALL references including snapshots!
 * ```
 * 
 * Best Practices:
 * 1. Use immutable data structures (Immer.js, Immutable.js)
 * 2. Always use setVariable() to update values, never modify objects directly
 * 3. If needed, manually deep clone before setting: setVariable("data", structuredClone(obj))
 * 4. Use freeze option in variable definition to prevent mutation: { name: "config", freeze: true }
 */
export class VariableManager implements StateManager<VariableManagerSnapshot> {
  /** All variables stored in a single flat Map */
  private variables: Map<string, VariableEntry> = new Map();

  /**
   * Optional reference to WorkflowExecutionEntity for runtime validation
   * This allows checking if accessed variables are declared in current node's variableInputs
   */
  private executionEntity?: {
    getCurrentNodeId?: () => string;
    getNode?: (nodeId: string) => { type: string; config?: { variableInputs?: Array<{ internalName: string }> } } | undefined;
  };

  /**
   * Optional cache for frequently accessed variables
   * Can be enabled for performance optimization
   */
  private cache: Map<string, { value: unknown; timestamp: number }> | null = null;
  private cacheEnabled: boolean = false;
  private cacheTTL: number = 1000; // 1 second TTL

  constructor(options?: { enableCache?: boolean; cacheTTL?: number }) {
    if (options?.enableCache) {
      this.cacheEnabled = true;
      this.cache = new Map();
      this.cacheTTL = options.cacheTTL || 1000;
    }
  }

  /**
   * Set the execution entity reference for runtime validation
   * @param entity WorkflowExecutionEntity instance
   */
  setExecutionEntity(entity: {
    getCurrentNodeId?: () => string;
    getNode?: (nodeId: string) => { type: string; config?: { variableInputs?: Array<{ internalName: string }> } } | undefined;
  }): void {
    this.executionEntity = entity;
  }



  /**
   * Initialize from VariableDefinition array
   * @param variableDefinitions Array of variable definitions
   */
  initializeFromDefinitions(variableDefinitions: VariableDefinition[]): void {
    logger.debug("Initializing from variable definitions", { count: variableDefinitions?.length || 0 });

    this.variables.clear();

    if (!variableDefinitions || variableDefinitions.length === 0) {
      return;
    }

    for (const definition of variableDefinitions) {
      this.registerVariable(definition);
    }
  }

  /**
   * Register a variable
   * @param definition Variable definition
   */
  registerVariable(definition: VariableDefinition): void {
    logger.debug("Registering variable", { 
      name: definition.name,
      freeze: definition.freeze 
    });

    const value = definition.value;
    
    // Auto-freeze if specified in definition
    if (definition.freeze && typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
      Object.freeze(value);
      logger.debug("Auto-froze variable value during registration", { name: definition.name });
    }

    const entry: VariableEntry = {
      definition,
      value,
    };

    // Store in the flat variables Map
    this.variables.set(definition.name, entry);
  }

  /**
   * Get variable value by name
   * 
   * Runtime Validation (development mode only):
   * - Checks if the variable is declared in current node's variableInputs
   * - Warns if accessing undeclared variables in subgraph/loop contexts
   * 
   * @param name Variable name
   * @returns Variable value or undefined if not found
   */
  getVariable(name: string): unknown {
    // Check cache first (if enabled)
    if (this.cacheEnabled && this.cache) {
      const cached = this.cache.get(name);
      if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
        logger.debug("Cache hit", { name });
        return cached.value;
      }
    }
    
    // Check variables Map
    if (this.variables.has(name)) {
      const value = this.variables.get(name)!.value;
      
      // Update cache
      if (this.cacheEnabled && this.cache) {
        this.cache.set(name, { value, timestamp: Date.now() });
      }
      
      return value;
    }
    
    // Runtime validation: warn about undeclared variable access (development mode only)
    if (process.env["NODE_ENV"] === "development" && this.executionEntity) {
      this.validateVariableAccess(name);
    }
    
    logger.debug("Variable not found", { name });
    return undefined;
  }

  /**
   * Validate variable access against current node's configuration (runtime check)
   * Only runs in development mode to catch programming errors early
   * @param name Variable name being accessed
   */
  private validateVariableAccess(name: string): void {
    try {
      const currentNodeId = this.executionEntity.getCurrentNodeId?.();
      if (!currentNodeId) return;
      
      const currentNode = this.executionEntity.getNode?.(currentNodeId);
      if (!currentNode) return;
      
      // Check if current node is a subgraph or loop that should have explicit inputs
      if (currentNode.type === "SUBGRAPH" || currentNode.type === "LOOP_START") {
        const declaredInputs = currentNode.config?.variableInputs || [];
        
        // Check if this variable was declared in variableInputs
        const isDeclared = declaredInputs.some(
          (input) => input.internalName === name
        );
        
        // Also check if it's a global variable (always accessible)
        // Note: In the new flat structure, all variables are in the same Map
        const isGlobal = this.variables.has(name);
        
        if (!isDeclared && !isGlobal) {
          logger.warn(
            `⚠️  Accessing undeclared variable '${name}' in ${currentNode.type} node '${currentNodeId}'.\n` +
            `This variable was not declared in variableInputs. It should have been caught by static validation.\n` +
            `Available variables: ${declaredInputs.map(
            (i) => i.internalName
          ).join(", ") || "(none)"}`
          );
        }
      }
    } catch (error) {
      // Silently ignore validation errors to avoid breaking production
      logger.debug("Runtime validation failed", { error });
    }
  }

  /**
   * Set variable value
   * 
   * @param name Variable name
   * @param value New value
   * @param freeze If true, freezes the value (overrides definition.freeze if provided)
   * @throws RuntimeValidationError if variable is readonly or doesn't exist
   */
  setVariable(name: string, value: unknown, freeze?: boolean): void {
    const entry = this.variables.get(name);
    
    if (!entry) {
      const availableVars = Array.from(this.variables.keys());
      throw new RuntimeValidationError(
        `Variable '${name}' is not defined. Available variables: ${availableVars.length > 0 ? availableVars.join(', ') : '(none)'}`,
        {
          operation: "setVariable",
          field: "variableName",
          value: name,
          context: { availableVariables: availableVars },
        }
      );
    }

    if (entry.definition.readonly) {
      throw new RuntimeValidationError(
        `Variable '${name}' is readonly and cannot be modified`,
        {
          operation: "setVariable",
          field: "variableName",
          value: name,
        }
      );
    }

    // Determine whether to freeze:
    // 1. Explicit parameter takes precedence
    // 2. Fall back to definition.freeze
    const shouldFreeze = freeze ?? entry.definition.freeze ?? false;

    logger.debug("Setting variable", { 
      name,
      freeze: shouldFreeze
    });

    if (shouldFreeze && typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
      Object.freeze(value);
      logger.debug("Froze object value for variable", { name });
    }

    // Update the value
    entry.value = value;

    // Invalidate cache
    if (this.cacheEnabled && this.cache) {
      this.cache.delete(name);
    }
  }

  /**
   * Check if variable exists
   * @param name Variable name
   * @returns true if variable is defined
   */
  hasVariable(name: string): boolean {
    return this.variables.has(name);
  }

  /**
   * Get variable definition
   * @param name Variable name
   * @returns Variable definition or undefined
   */
  getVariableDefinition(name: string): VariableDefinition | undefined {
    return this.variables.get(name)?.definition;
  }

  /**
   * Get all variable definitions
   * @returns Array of all variable definitions
   */
  getAllVariableDefinitions(): VariableDefinition[] {
    const definitions: VariableDefinition[] = [];
    for (const entry of this.variables.values()) {
      definitions.push(entry.definition);
    }
    return definitions;
  }

  /**
   * Get all variables
   * @returns Record of all variable key-value pairs
   */
  getAllVariables(): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const [name] of this.variables) {
      const value = this.getVariable(name);
      if (value !== undefined) {
        result[name] = value;
      }
    }

    return result;
  }


  /**
   * Delete variable
   * @param name Variable name
   * @returns true if deleted successfully
   */
  deleteVariable(name: string): boolean {
    logger.debug("Deleting variable", { name });

    const deleted = this.variables.delete(name);

    // Invalidate cache
    if (this.cacheEnabled && this.cache) {
      this.cache.delete(name);
    }

    return deleted;
  }

  /**
   * Create snapshot (for checkpoints)
   * @returns Complete state snapshot
   */
  createSnapshot(): VariableManagerSnapshot {
    return {
      variables: new Map(this.variables),
    };
  }

  /**
   * Restore from snapshot (for checkpoints)
   * @param snapshot State snapshot
   */
  restoreFromSnapshot(snapshot: VariableManagerSnapshot): void {
    this.variables = new Map(snapshot.variables);

    // Clear cache after restore
    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }
  }

  /**
   * Copy from another VariableManager (for fork scenarios)
   * @param source Source VariableManager
   * 
   * Note: 
   * - All variables are DEEP CLONED to ensure complete isolation
   * - This eliminates implicit state sharing between fork branches
   * - For explicit variable passing with custom mapping, use importVariables() instead.
   */
  copyFrom(source: VariableManager): void {
    // Deep clone all variables to ensure complete isolation
    this.variables = new Map();
    for (const [name, entry] of source.variables) {
      try {
        // Deep clone the value to prevent shared state
        const clonedValue = structuredClone(entry.value);
        this.variables.set(name, {
          definition: { ...entry.definition },
          value: clonedValue,
        });
      } catch (error) {
        // Fallback to shallow copy if structuredClone fails
        logger.warn("structuredClone failed in copyFrom, using shallow copy", {
          variableName: name,
          error
        });
        this.variables.set(name, {
          definition: { ...entry.definition },
          value: entry.value,
        });
      }
    }

    // Clear cache
    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }
    
    logger.debug("VariableManager copied from source with complete deep clone", {
      variableCount: this.variables.size,
    });
  }

  /**
   * Import variables from parent workflow with explicit mapping and deep clone
   * 
   * This is the ONLY way to receive variables from a parent workflow.
   * All imported variables are deep cloned to ensure complete isolation.
   * 
   * @param source Source VariableManager (parent)
   * @param mappings Explicit variable input mappings
   * @throws RuntimeValidationError if required variable is not found in source
   * 
   * Example:
   * ```typescript
   * childManager.importVariables(parentManager, [
   *   { externalName: 'user_id', internalName: 'uid', required: true },
   *   { externalName: 'config', internalName: 'settings', defaultValue: {} }
   * ]);
   * ```
   */
  importVariables(
    source: VariableManager,
    mappings: WorkflowVariableInput[]
  ): void {
    logger.debug("Importing variables from parent", { count: mappings.length });
    
    for (const mapping of mappings) {
      const value = source.getVariable(mapping.externalName);
      
      if (value === undefined) {
        if (mapping.required) {
          throw new RuntimeValidationError(
            `Required input variable '${mapping.externalName}' not found in parent workflow`,
            {
              operation: "importVariables",
              field: mapping.externalName,
              context: {
                externalName: mapping.externalName,
                internalName: mapping.internalName,
                required: mapping.required
              }
            }
          );
        }
        // Use default value if provided
        if (mapping.defaultValue !== undefined) {
          // Deep clone default value to avoid shared references
          const clonedDefault = structuredClone(mapping.defaultValue);
          // Auto-register variable during import
          this.registerVariable({
            name: mapping.internalName,
            type: inferVariableType(clonedDefault),
            value: clonedDefault,
            readonly: false,
          });
          logger.debug("Used default value for optional input", {
            internalName: mapping.internalName
          });
        }
      } else {
        // Always deep clone to ensure isolation
        try {
          const clonedValue = structuredClone(value);
          // Auto-register variable during import
          this.registerVariable({
            name: mapping.internalName,
            type: inferVariableType(clonedValue),
            value: clonedValue,
            readonly: false,
          });
          logger.debug("Imported variable with deep clone", {
            externalName: mapping.externalName,
            internalName: mapping.internalName
          });
        } catch (error) {
          // Handle cases where structuredClone fails (e.g., functions, DOM nodes)
          logger.warn("structuredClone failed, using shallow copy", {
            externalName: mapping.externalName,
            error
          });
          // Auto-register variable during import
          this.registerVariable({
            name: mapping.internalName,
            type: inferVariableType(value),
            value: value,
            readonly: false,
          });
        }
      }
    }
  }

  /**
   * Export variables to parent workflow with explicit mapping and deep clone
   * 
   * This is the ONLY way to return variables to a parent workflow.
   * All exported variables are deep cloned before transfer.
   * 
   * @param target Target VariableManager (parent)
   * @param mappings Explicit variable output mappings
   * 
   * Example:
   * ```typescript
   * childManager.exportVariables(parentManager, [
   *   { internalName: 'result', externalName: 'output' },
   *   { internalName: 'status', externalName: 'execution_status' }
   * ]);
   * ```
   */
  exportVariables(
    target: VariableManager,
    mappings: WorkflowVariableOutput[]
  ): void {
    logger.debug("Exporting variables to parent", { count: mappings.length });
    
    for (const mapping of mappings) {
      const value = this.getVariable(mapping.internalName);
      
      if (value !== undefined) {
        try {
          // Deep clone before exporting
          const clonedValue = structuredClone(value);
          // Auto-register variable in target if not exists
          if (!target.hasVariable(mapping.externalName)) {
            target.registerVariable({
              name: mapping.externalName,
              type: inferVariableType(clonedValue),
              value: clonedValue,
              readonly: false,
            });
          } else {
            target.setVariable(mapping.externalName, clonedValue);
          }
          logger.debug("Exported variable with deep clone", {
            internalName: mapping.internalName,
            externalName: mapping.externalName
          });
        } catch (error) {
          // Handle cases where structuredClone fails
          logger.warn("structuredClone failed during export, using shallow copy", {
            internalName: mapping.internalName,
            error
          });
          // Auto-register variable in target if not exists
          if (!target.hasVariable(mapping.externalName)) {
            target.registerVariable({
              name: mapping.externalName,
              type: inferVariableType(value),
              value: value,
              readonly: false,
            });
          } else {
            target.setVariable(mapping.externalName, value);
          }
        }
      } else {
        // Variable doesn't exist - silently skip (optional outputs)
        logger.debug("Skipping undefined output variable", {
          internalName: mapping.internalName,
          externalName: mapping.externalName
        });
      }
    }
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    this.variables.clear();

    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }

    logger.debug("VariableManager cleaned up");
  }

  /**
   * Get the number of variables
   * @returns Count of registered variables
   */
  size(): number {
    return this.variables.size;
  }

  /**
   * Check if empty
   * @returns true if no variables registered
   */
  isEmpty(): boolean {
    return this.variables.size === 0;
  }

  /**
   * Reset to initial state
   */
  reset(): void {
    this.cleanup();
  }

  /**
   * Enable/disable cache
   * @param enable Whether to enable cache
   * @param ttl Cache TTL in milliseconds
   */
  setCache(enable: boolean, ttl?: number): void {
    this.cacheEnabled = enable;
    if (enable) {
      this.cache = new Map();
      this.cacheTTL = ttl || 1000;
    } else {
      this.cache = null;
    }
  }
}
