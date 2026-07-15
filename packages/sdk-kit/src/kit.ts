/**
 * SDKKit - Main entry point for SDK-Kit
 *
 * Provides simplified, high-level APIs for common workflow scenarios.
 * Phase 1 focuses on core functionality: workflow definition, execution, and querying.
 */

import { WorkflowAPIImpl } from './api/workflow.api.js';
import { ExecutionAPIImpl } from './executors/execution.executor.js';
import { QueryAPIImpl } from './executors/query.executor.js';
import { ResourceAPIImpl } from './api/resource.api.js';
import { ExecutionRunner } from './executors/execution.executor.js';
import { QueryExecutor } from './executors/query.executor.js';
import { ResourceManager } from './managers/resource.manager.js';
import { EventManager } from './managers/event.manager.js';
import { ComparisonAnalysis, ProgressAnalysis } from './analysis/index.js';
import { KitError, KitErrorCode } from './converters/error.converter.js';
import { PluginManager } from './plugin/plugin.manager.js';
import type { WorkflowAPI } from './api/workflow.api.js';
import type { ExecutionAPI } from './api/execution.api.js';
import type { QueryAPI } from './api/query.api.js';
import type { ResourceAPI } from './types/resource.types.js';
import type { SDK, ExecuteWorkflowCommandConstructor } from './types/sdk.types.js';
import type { SDKKitOptions } from './types/options.types.js';
import { mergeSDKKitOptions } from './types/options.types.js';

/**
 * SDKKit - Main class providing high-level API access
 *
 * Core APIs:
 * - workflow(): WorkflowAPI - Define workflows programmatically
 * - execution(): ExecutionAPI - Execute workflows with simplified interface
 * - query(): QueryAPI - Query execution records with filters
 * - resource(): ResourceAPI - Manage workflow resources (CRUD + versioning)
 * - events(): EventManager - Subscribe to execution events
 * - getConfig(): Get current configuration
 */
export class SDKKit {
  private workflowAPI: WorkflowAPI;
  private executionAPI: ExecutionAPI;
  private queryAPI: QueryAPI;
  private resourceAPI: ResourceAPI;
  private executionRunner: ExecutionRunner;
  private queryExecutor: QueryExecutor;
  private resourceManager: ResourceManager;
  private eventManager: EventManager;
  private comparisonAnalysis?: ComparisonAnalysis;
  private progressAnalysis?: ProgressAnalysis;
  private cachedExecuteCommand: ExecuteWorkflowCommandConstructor;
  private sdk: SDK;
  private config: Required<SDKKitOptions>;
  private _pluginManager?: PluginManager;

  constructor(sdk: any, options?: SDKKitOptions) {
    // Validate SDK instance before using it
    this.validateSDK(sdk);

    // Store the SDK instance
    this.sdk = sdk;

    // Merge user options with defaults
    this.config = mergeSDKKitOptions(options);

    // Cache the ExecuteWorkflowCommand class to avoid repeated imports
    this.cachedExecuteCommand = this.cacheExecuteCommand(sdk);

    // Initialize event manager for execution event monitoring
    this.eventManager = new EventManager();

    // Apply configuration to EventManager
    this.applyEventConfig();

    // Initialize managers
    this.executionRunner = new ExecutionRunner(sdk, this.cachedExecuteCommand);
    this.queryExecutor = new QueryExecutor(sdk);
    this.resourceManager = new ResourceManager(sdk);

    // Initialize public APIs
    this.workflowAPI = new WorkflowAPIImpl();
    this.executionAPI = new ExecutionAPIImpl(this.executionRunner);
    this.queryAPI = new QueryAPIImpl(this.queryExecutor);
    this.resourceAPI = new ResourceAPIImpl(this.resourceManager);

    // Initialize advanced APIs
    const deps = sdk.getFactory()?.getDependencies?.();
    if (deps) {
      this.comparisonAnalysis = new ComparisonAnalysis(sdk);
      this.progressAnalysis = new ProgressAnalysis(sdk);
    }
  }

  /**
   * Apply event configuration to EventManager
   */
  private applyEventConfig(): void {
    if (this.config.events?.enableHistory === false) {
      this.eventManager.setMaxHistorySize(0);
    } else if (this.config.events?.maxHistorySize) {
      this.eventManager.setMaxHistorySize(this.config.events.maxHistorySize);
    }
  }

  /**
   * Validate SDK instance has required methods and properties
   * Enhanced validation (Phase 1.3) with stricter checks
   */
  private validateSDK(sdk: any): asserts sdk is SDK {
    if (!sdk || typeof sdk !== 'object') {
      throw new KitError(
        'Invalid SDK instance - must be an object',
        KitErrorCode.INTERNAL_ERROR
      );
    }

    // Check for required methods
    if (typeof sdk.executeCommand !== 'function') {
      throw new KitError(
        'SDK missing executeCommand method',
        KitErrorCode.INTERNAL_ERROR
      );
    }

    if (typeof sdk.getFactory !== 'function') {
      throw new KitError(
        'SDK missing getFactory method',
        KitErrorCode.INTERNAL_ERROR
      );
    }

    // Validate SDK factory
    try {
      const factory = sdk.getFactory();
      if (!factory || typeof factory !== 'object') {
        throw new KitError(
          'SDK.getFactory() must return a valid factory object',
          KitErrorCode.INTERNAL_ERROR
        );
      }
      if (typeof factory.getDependencies !== 'function') {
        throw new KitError(
          'SDK factory must have getDependencies method',
          KitErrorCode.INTERNAL_ERROR
        );
      }
    } catch (error) {
      if (error instanceof KitError) throw error;
      throw new KitError(
        `Failed to validate SDK factory: ${error instanceof Error ? error.message : 'Unknown error'}`,
        KitErrorCode.INTERNAL_ERROR
      );
    }

    // Validate SDK version if available (Phase 1.3 enhancement)
    if (sdk.version !== undefined) {
      if (typeof sdk.version !== 'string') {
        throw new KitError(
          `SDK version must be a string, got ${typeof sdk.version}`,
          KitErrorCode.INTERNAL_ERROR
        );
      }
      if (!this.isCompatibleVersion(sdk.version)) {
        throw new KitError(
          `SDK version ${sdk.version} not compatible. Required: 1.0.0+`,
          KitErrorCode.INTERNAL_ERROR
        );
      }
    }

    // Validate ExecuteWorkflowCommand availability (stricter)
    const hasExecuteCommand = !!sdk.ExecuteWorkflowCommand || !!sdk.api?.ExecuteWorkflowCommand;
    if (!hasExecuteCommand) {
      throw new KitError(
        'ExecuteWorkflowCommand not found in SDK exports (must be at sdk.ExecuteWorkflowCommand or sdk.api.ExecuteWorkflowCommand)',
        KitErrorCode.INTERNAL_ERROR
      );
    }
  }

  /**
   * Check if SDK version is compatible (major version >= 1)
   * Used during Phase 1.3 validation enhancement
   */
  private isCompatibleVersion(version?: string): boolean {
    if (!version || typeof version !== 'string') {
      return true; // Allow undefined or non-string versions (legacy SDKs)
    }
    try {
      const parts = version.split('.');
      const majorStr = parts?.[0];
      if (!majorStr) return false;
      const major = parseInt(majorStr, 10);
      return major >= 1;
    } catch {
      return false;
    }
  }

  /**
   * Cache ExecuteWorkflowCommand from SDK to avoid repeated imports
   */
  private cacheExecuteCommand(sdk: SDK): ExecuteWorkflowCommandConstructor {
    // Try direct property first
    if (sdk.ExecuteWorkflowCommand) {
      return sdk.ExecuteWorkflowCommand;
    }

    // Try nested api property
    if (sdk.api?.ExecuteWorkflowCommand) {
      return sdk.api.ExecuteWorkflowCommand;
    }

    throw new KitError(
      'ExecuteWorkflowCommand not found in SDK exports',
      KitErrorCode.INTERNAL_ERROR
    );
  }

  /**
   * Get Workflow API for defining workflows programmatically
   *
   * @example
   * ```typescript
   * const template = kit.workflow()
   *   .create('my-workflow')
   *   .node('start', { type: 'START' })
   *   .node('task', { type: 'LLM' })
   *   .edge('start', 'task')
   *   .build();
   * ```
   */
  workflow(): WorkflowAPI {
    return this.workflowAPI;
  }

  /**
   * Get Execution API for executing workflows
   *
   * @example
   * ```typescript
   * const result = await kit.execution()
   *   .workflow('my-workflow')
   *   .input({ data: 'test' })
   *   .execute();
   * ```
   */
  execution(): ExecutionAPI {
    return this.executionAPI;
  }

  /**
   * Get Query API for querying execution records
   *
   * @example
   * ```typescript
   * const executions = await kit.query()
   *   .executions()
   *   .filter({ status: 'completed' })
   *   .limit(10)
   *   .get();
   * ```
   */
  query(): QueryAPI {
    return this.queryAPI;
  }

  /**
   * Get Resource API for managing workflow resources
   *
   * @example
   * ```typescript
   * // Create workflow
   * const id = await kit.resource()
   *   .workflows()
   *   .create({ id: 'wf1', nodes: [...], edges: [...] });
   *
   * // Get workflow
   * const workflow = await kit.resource()
   *   .workflows()
   *   .read(id);
   *
   * // Update workflow
   * await kit.resource()
   *   .workflows()
   *   .update(id, { description: 'Updated' });
   *
   * // Delete workflow
   * await kit.resource()
   *   .workflows()
   *   .delete(id);
   * ```
   */
  resource(): ResourceAPI {
    return this.resourceAPI;
  }

  /**
   * Get Execution Event Manager for subscribing to execution events
   *
   * Provides access to workflow execution events including:
   * - Execution lifecycle events (start, progress, completed, failed)
   * - Node execution events (node_started, node_completed, node_failed)
   * - Event history and timeline queries
   *
   * @example
   * ```typescript
   * import { ExecutionEventType } from '@wf-agent/sdk-kit';
   *
   * // Subscribe to execution events
   * kit.events().subscribe(
   *   ExecutionEventType.EXECUTION_START,
   *   (event) => console.log('Execution started:', event.executionId)
   * );
   *
   * // Query event history
   * const events = kit.events().getExecutionEvents(executionId);
   * events.forEach(event => {
   *   console.log(`[${event.type}] ${event.timestamp}`);
   * });
   * ```
   *
   * @returns EventManager instance for event subscription and history queries
   */
  events(): EventManager {
    return this.eventManager;
  }

  /**
   * Get Analysis APIs for comparing and analyzing executions
   *
   * Provides analysis capabilities:
   * - Compare executions (performance, errors, interruptions)
   * - Track execution progress with time estimation
   *
   * @example
   * ```typescript
   * const comparison = await kit.analysis().comparison
   *   .compare(exec1Id, exec2Id);
   *
   * const progress = kit.analysis().progress
   *   .createTracker(executionId);
   * ```
   */
  analysis() {
    return {
      comparison: this.comparisonAnalysis,
      progress: this.progressAnalysis,
    };
  }

  /**
   * Access the plugin manager.
   * Only available if the SDK was created with plugins enabled.
   *
   * @example
   * ```typescript
   * const sdk = createSDK({ plugins: { enabled: true } });
   * const kit = new SDKKit(sdk);
   *
   * // Load a plugin
   * await kit.plugins().loadPlugin('@scope/my-plugin');
   *
   * // List active plugins
   * const active = kit.plugins().list().filter(p => p.status === 'active');
   * ```
   */
  plugins(): PluginManager {
    if (!this._pluginManager) {
      this._pluginManager = new PluginManager(this.sdk);
    }
    return this._pluginManager;
  }

  /**
   * Get current configuration
   * Returns read-only copy of configuration
   *
   * @example
   * ```typescript
   * const config = kit.getConfig();
   * console.log(config.events.maxHistorySize);  // 10000
   * console.log(config.logging.level);          // 'info'
   * ```
   *
   * @returns Readonly copy of the current SDK-Kit configuration
   */
  getConfig(): Readonly<Required<SDKKitOptions>> {
    return Object.freeze(structuredClone(this.config));
  }

  /**
   * Get cached ExecuteWorkflowCommand class
   * Used internally by ExecutionRunner to avoid repeated imports
   */
  getExecuteCommand(): ExecuteWorkflowCommandConstructor {
    return this.cachedExecuteCommand;
  }

  /**
   * Get the underlying SDK instance for advanced use cases
   *
   * Use this for scenarios not covered by the high-level API.
   * Most applications should use the workflow(), execution(), and query() methods instead.
   */
  getSDK(): SDK {
    return this.sdk;
  }
}

export default SDKKit;
