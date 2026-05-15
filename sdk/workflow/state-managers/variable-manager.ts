/**
 * VariableManager - Simplified Variable State Manager (REFACTORED)
 * 
 * Inspired by MessageHistory design philosophy:
 * - Single source of truth: Two Maps for global and execution scopes
 * - Unified access interface: getVariable() / setVariable()
 * - Scope stack for temporary isolation in subgraphs and loops
 * 
 * Key Features:
 * - Simplified structure: Global and execution scopes for persistent variables
 * - Temporary scope stack: For subgraph and loop variable isolation
 * - Explicit variable mapping: Variables must be explicitly passed between scopes
 * - Automatic cleanup: Scope exit discards all local variables
 * 
 * Design Principles:
 * - Simplicity: Follow MessageHistory's flat structure approach
 * - Consistency: Unified API for all operations
 * - Performance: O(1) lookup with optional caching
 * - Isolation: Scope stack provides proper variable isolation for nested contexts
 * 
 * Architecture Changes:
 * - Removed variableOutputs from START node (START is entry-only)
 * - Added scopeStack for dynamic variable isolation
 * - Variable mapping happens at graph build time, not runtime
 * - Subgraph variables are isolated and automatically cleaned up on exit
 */

import type { VariableDefinition, VariableScope } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { StateManager } from "../../core/types/state-manager.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "variable-manager" });

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
  global: Map<string, VariableEntry>;
  execution: Map<string, VariableEntry>;
  scopeStack: Array<Map<string, VariableEntry>>;
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
 * 
 * Usage Example:
 * ```typescript
 * const manager = new VariableManager();
 * 
 * // Register variables with freeze
 * manager.registerVariable({
 *   name: 'config',
 *   type: 'object',
 *   value: { timeout: 5000 },
 *   scope: 'global',
 *   readonly: true,
 *   freeze: true  // Auto-freeze on registration
 * });
 * 
 * // Get/Set variables
 * manager.setVariable('counter', 5);
 * const value = manager.getVariable('counter'); // 5
 * 
 * // Enter/Exit scopes
 * manager.enterSubgraphScope();
 * manager.setVariable('temp', 'value');
 * manager.exitSubgraphScope();
 * ```
 */
export class VariableManager implements StateManager<VariableManagerSnapshot> {
  /** Global scope - shared across all executions */
  private global: Map<string, VariableEntry> = new Map();
  
  /** Execution scope - isolated per execution */
  private execution: Map<string, VariableEntry> = new Map();
  
  /** Scope stack - for temporary isolation in subgraphs and loops */
  private scopeStack: Array<Map<string, VariableEntry>> = [];

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
   * Enter a new scope (for subgraphs or loops)
   * Creates a temporary isolated scope on top of the scope stack
   * 
   * Usage Example:
   * ```typescript
   * manager.enterSubgraphScope();
   * manager.setVariable('temp', 'value'); // Stored in current scope
   * manager.exitSubgraphScope();
   * ```
   */
  enterSubgraphScope(): void {
    logger.debug("Entering new scope", { depth: this.scopeStack.length + 1 });
    this.scopeStack.push(new Map());
  }
  
  /**
   * Exit the current scope (for subgraphs or loops)
   * Removes the topmost scope from the stack
   * All variables in that scope are discarded
   * 
   * Note: If you need to preserve variables, copy them before exiting
   */
  exitSubgraphScope(): void {
    if (this.scopeStack.length > 0) {
      const removedScope = this.scopeStack.pop();
      logger.debug("Exiting scope", { 
        depth: this.scopeStack.length,
        variablesDiscarded: removedScope?.size || 0
      });
    } else {
      logger.warn("Attempted to exit scope but no scope is active");
    }
  }

  /**
   * Initialize from VariableDefinition array
   * @param variableDefinitions Array of variable definitions
   */
  initializeFromDefinitions(variableDefinitions: VariableDefinition[]): void {
    logger.debug("Initializing from variable definitions", { count: variableDefinitions?.length || 0 });

    this.global.clear();
    this.execution.clear();

    if (!variableDefinitions || variableDefinitions.length === 0) {
      return;
    }

    for (const definition of variableDefinitions) {
      this.registerVariable(definition);
    }
  }

  /**
   * Register a variable (similar to MessageHistory.addMessage)
   * @param definition Variable definition
   */
  registerVariable(definition: VariableDefinition): void {
    logger.debug("Registering variable", { 
      name: definition.name, 
      scope: definition.scope,
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

    // Store in appropriate scope based on definition
    if (definition.scope === "global") {
      this.global.set(definition.name, entry);
    } else {
      // All other scopes (execution, subgraph, loop) go to execution scope
      this.execution.set(definition.name, entry);
    }
  }

  /**
   * Get variable value by name (unified entry point)
   * Priority: current scope (deepest) > execution > global
   * 
   * Search order:
   * 1. Current scope (top of scopeStack) if exists
   * 2. Execution scope
   * 3. Global scope
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

    // Check current scope first (if scope stack exists)
    if (this.scopeStack.length > 0) {
      for (let i = this.scopeStack.length - 1; i >= 0; i--) {
        const scope = this.scopeStack[i];
        if (scope && scope.has(name)) {
          const entry = scope.get(name);
          if (entry) {
            const value = entry.value;
            
            // Update cache
            if (this.cacheEnabled && this.cache) {
              this.cache.set(name, { value, timestamp: Date.now() });
            }
            
            return value;
          }
        }
      }
    }
    
    // Check execution scope
    if (this.execution.has(name)) {
      const value = this.execution.get(name)!.value;
      
      // Update cache
      if (this.cacheEnabled && this.cache) {
        this.cache.set(name, { value, timestamp: Date.now() });
      }
      
      return value;
    }
    
    // Check global scope
    if (this.global.has(name)) {
      const value = this.global.get(name)!.value;
      
      // Update cache
      if (this.cacheEnabled && this.cache) {
        this.cache.set(name, { value, timestamp: Date.now() });
      }
      
      return value;
    }
    
    logger.debug("Variable not found", { name });
    return undefined;
  }

  /**
   * Set variable value (unified entry point)
   * Variables are stored in the current deepest scope (if any), otherwise in execution scope
   * Use explicit API to set global variables
   * 
   * Priority for storage:
   * 1. Current scope (top of scopeStack) if exists
   * 2. Execution scope otherwise
   * 
   * @param name Variable name
   * @param value New value
   * @param freeze If true, freezes the value (overrides definition.freeze if provided)
   * @throws RuntimeValidationError if variable is readonly or doesn't exist
   */
  setVariable(name: string, value: unknown, freeze?: boolean): void {
    let entry: VariableEntry | undefined;
    let targetMap: Map<string, VariableEntry> | undefined;
    
    // Determine which scope to use
    if (this.scopeStack.length > 0) {
      // Use the current deepest scope
      const currentScope = this.scopeStack[this.scopeStack.length - 1];
      if (currentScope) {
        targetMap = currentScope;
        entry = targetMap.get(name);
      }
      
      // If not found in current scope, check execution then global
      if (!entry) {
        entry = this.execution.get(name);
      }
      if (!entry) {
        entry = this.global.get(name);
      }
    } else {
      // No active scope, use execution scope
      entry = this.execution.get(name);
      
      // If not found in execution, check global
      if (!entry) {
        entry = this.global.get(name);
      }
    }
    
    if (!entry) {
      const availableVars = [
        ...Array.from(this.execution.keys()),
        ...Array.from(this.global.keys()),
        ...this.scopeStack.flatMap(scope => Array.from(scope.keys()))
      ];
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
      scope: entry.definition.scope, 
      freeze: shouldFreeze,
      inScopeStack: this.scopeStack.length > 0
    });

    if (shouldFreeze && typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
      Object.freeze(value);
      logger.debug("Froze object value for variable", { name });
    }

    // Update the value in the appropriate scope
    const currentScope = this.scopeStack.length > 0 ? this.scopeStack[this.scopeStack.length - 1] : undefined;
    if (currentScope && currentScope.has(name)) {
      // Update in current scope if it exists there
      const entry = currentScope.get(name);
      if (entry) {
        entry.value = value;
      }
    } else if (this.execution.has(name)) {
      // Update in execution scope
      const entry = this.execution.get(name);
      if (entry) {
        entry.value = value;
      }
    } else if (this.global.has(name)) {
      // Update in global scope
      const entry = this.global.get(name);
      if (entry) {
        entry.value = value;
      }
    } else {
      // Create new entry in the appropriate scope
      const newEntry: VariableEntry = {
        definition: entry.definition,
        value
      };
      
      if (this.scopeStack.length > 0) {
        const currentScope = this.scopeStack[this.scopeStack.length - 1];
        if (currentScope) {
          currentScope.set(name, newEntry);
        }
      } else {
        this.execution.set(name, newEntry);
      }
    }

    // Invalidate cache
    if (this.cacheEnabled && this.cache) {
      this.cache.delete(name);
    }
  }

  /**
   * Check if variable exists
   * Checks all scopes: current scope stack, execution, and global
   * @param name Variable name
   * @returns true if variable is defined
   */
  hasVariable(name: string): boolean {
    // Check scope stack first
    if (this.scopeStack.length > 0) {
      for (const scope of this.scopeStack) {
        if (scope.has(name)) {
          return true;
        }
      }
    }
    
    return this.execution.has(name) || this.global.has(name);
  }

  /**
   * Get variable definition
   * Searches through all scopes to find the variable definition
   * @param name Variable name
   * @returns Variable definition or undefined
   */
  getVariableDefinition(name: string): VariableDefinition | undefined {
    // Check scope stack first
    if (this.scopeStack.length > 0) {
      for (const scope of this.scopeStack) {
        const entry = scope.get(name);
        if (entry) {
          return entry.definition;
        }
      }
    }
    
    return this.execution.get(name)?.definition || this.global.get(name)?.definition;
  }

  /**
   * Get all variable definitions
   * @returns Array of all variable definitions
   */
  getAllVariableDefinitions(): VariableDefinition[] {
    const definitions: VariableDefinition[] = [];
    for (const entry of this.execution.values()) {
      definitions.push(entry.definition);
    }
    for (const entry of this.global.values()) {
      definitions.push(entry.definition);
    }
    return definitions;
  }

  /**
   * Get all variables (merged by scope priority)
   * @returns Record of all accessible variable key-value pairs
   */
  getAllVariables(): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    // Add global variables first
    for (const [name] of this.global) {
      const value = this.getVariable(name);
      if (value !== undefined) {
        result[name] = value;
      }
    }

    // Add execution variables (may override global)
    for (const [name] of this.execution) {
      const value = this.getVariable(name);
      if (value !== undefined) {
        result[name] = value;
      }
    }

    return result;
  }

  /**
   * Get variables by scope
   * @param scope Variable scope
   * @returns Record of variables in that scope
   */
  getVariablesByScope(scope: VariableScope): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    // For backward compatibility - map old scopes to new structure
    const targetMap = scope === "global" ? this.global : this.execution;

    for (const [name] of targetMap) {
      const value = this.getVariable(name);
      if (value !== undefined) {
        result[name] = value;
      }
    }

    return result;
  }

  /**
   * Delete variable
   * Tries to delete from all scopes: current scope stack, execution, then global
   * @param name Variable name
   * @returns true if deleted successfully
   */
  deleteVariable(name: string): boolean {
    logger.debug("Deleting variable", { name });

    let deleted = false;
    
    // Try to delete from scope stack first
    if (this.scopeStack.length > 0) {
      for (const scope of this.scopeStack) {
        if (scope.delete(name)) {
          deleted = true;
        }
      }
    }
    
    // Try to delete from execution and global scopes
    const deletedFromExecution = this.execution.delete(name);
    const deletedFromGlobal = this.global.delete(name);

    // Invalidate cache
    if (this.cacheEnabled && this.cache) {
      this.cache.delete(name);
    }

    return deleted || deletedFromExecution || deletedFromGlobal;
  }

  /**
   * Create snapshot (for checkpoints)
   * Includes scope stack state for complete state capture
   * @returns Complete state snapshot
   */
  createSnapshot(): VariableManagerSnapshot {
    return {
      global: new Map(this.global),
      execution: new Map(this.execution),
      scopeStack: this.scopeStack.map(scope => new Map(scope)),
    };
  }

  /**
   * Restore from snapshot (for checkpoints)
   * Restores all scopes including the scope stack
   * @param snapshot State snapshot
   */
  restoreFromSnapshot(snapshot: VariableManagerSnapshot): void {
    this.global = new Map(snapshot.global);
    this.execution = new Map(snapshot.execution);
    this.scopeStack = snapshot.scopeStack.map(scope => new Map(scope));

    // Clear cache after restore
    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }
  }

  /**
   * Copy from another VariableManager (for fork scenarios)
   * @param source Source VariableManager
   * 
   * Note: Global scope variables are shared by reference
   * Execution scope variables are deep copied
   */
  copyFrom(source: VariableManager): void {
    // Share global variables by reference
    this.global = source.global;
    
    // Deep copy execution variables
    this.execution = new Map();
    for (const [name, entry] of source.execution) {
      this.execution.set(name, {
        definition: { ...entry.definition },
        value: entry.value,
      });
    }

    // Clear cache
    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }
  }

  /**
   * Clean up resources
   * Clears all scopes including the scope stack
   */
  cleanup(): void {
    this.global.clear();
    this.execution.clear();
    this.scopeStack = [];

    if (this.cacheEnabled && this.cache) {
      this.cache.clear();
    }

    logger.debug("VariableManager cleaned up");
  }

  /**
   * Get the number of variables across all scopes
   * @returns Count of registered variables
   */
  size(): number {
    const scopeStackCount = this.scopeStack.reduce((sum, scope) => sum + scope.size, 0);
    return this.global.size + this.execution.size + scopeStackCount;
  }

  /**
   * Check if empty
   * @returns true if no variables registered in any scope
   */
  isEmpty(): boolean {
    return this.global.size === 0 && 
           this.execution.size === 0 && 
           this.scopeStack.every(scope => scope.size === 0);
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
