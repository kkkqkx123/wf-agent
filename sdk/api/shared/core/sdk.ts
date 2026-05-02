/**
 * SDK Main Class
 * Provides a unified API entry point that integrates all functional modules.
 *
 * Reconstruction Notes:
 * - Use APIFactory to centrally manage the creation of API instances.
 * - Simplify the configuration process and remove unnecessary configuration mechanisms.
 * - Support the application layer to implement the CheckpointStorageAdapter interface.
 */

import { APIFactory } from "./api-factory.js";
import { APIDependencyManager } from "./sdk-dependencies.js";
import { getData } from "../types/execution-result.js";
import type { SDKOptions } from "../types/core-types.js";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import {
  getContainer,
} from "../../../core/di/container-config.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { registerContextCompression } from "../../../resources/predefined/index.js";
import { registerPredefinedPromptTemplates } from "../../../resources/predefined/prompts/index.js";
import { registerAllPredefinedContent } from "../../../resources/predefined/registration.js";
import { TomlParserManager } from "../../../utils/toml-parser-manager.js";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type { TriggerTemplateRegistry } from "../../../core/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "../../../workflow/stores/workflow-registry.js";
import { StorageInitializationService } from "../../../core/services/storage-initialization-service.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";

/**
 * SDK Main Class - Unified API Entry Point (Internal class, not exported)
 * 
 * Architecture Principles:
 * - SDK provides the mechanism for initialization
 * - Apps provide the policy through configuration options
 * - Sensible defaults with opt-out capability for presets
 * - Dependency injection pattern for storage adapters
 * 
 * Initialization Flow:
 * 1. Constructor stores options and starts async bootstrap
 * 2. Bootstrap initializes storage adapters (if provided)
 * 3. Bootstrap preloads internal modules (TomlParserManager)
 * 4. Bootstrap registers presets based on config (context compression, tools, prompts)
 * 5. Apps can await waitForReady() for explicit control
 */
class SDK {
  private factory: APIFactory;
  private dependencies: APIDependencyManager;
  private bootstrapPromise?: Promise<void>;
  private isBootstrapped: boolean = false;

  /**
   * Create an SDK instance
   * @param options SDK configuration options
   */
  constructor(options?: SDKOptions) {
    // Store options for later initialization in bootstrap
    this.pendingOptions = options;
    
    // Initialize the API factory.
    this.factory = APIFactory.getInstance();
    // Initialize the dependency manager
    this.dependencies = new APIDependencyManager();

    // Initialize preload content (executed asynchronously, without blocking the constructor)
    this.bootstrapPromise = this.bootstrap(options).catch(error => {
      logger.error(`Failed to bootstrap SDK: ${getErrorMessage(error)}`);
      // Call error hook if provided
      options?.hooks?.onBootstrapError?.(error);
    });
  }
  
  /**
   * Pending options stored for bootstrap phase
   */
  private pendingOptions?: SDKOptions;

  /**
   * Initialize preset functionality
   * @param options SDK configuration options
   */
  private async bootstrap(options?: SDKOptions): Promise<void> {
    // Call start hook if provided
    await options?.hooks?.onBootstrapStart?.();

    // Step 1: Initialize storage adapters through StorageInitializationService
    try {
      const storageService = StorageInitializationService.getInstance();
      
      // Only initialize if adapters are provided
      if (options?.checkpointStorageAdapter || options?.taskStorageAdapter || options?.workflowStorageAdapter || options?.workflowExecutionStorageAdapter || options?.agentLoopCheckpointStorageAdapter) {
        await storageService.initialize({
          checkpoint: options.checkpointStorageAdapter,
          task: options.taskStorageAdapter,
          workflow: options.workflowStorageAdapter,
          workflowExecution: options.workflowExecutionStorageAdapter,
          agentLoopCheckpoint: options.agentLoopCheckpointStorageAdapter,
        });
        
        logger.info("Storage adapters initialized via StorageInitializationService", {
          checkpoint: !!options.checkpointStorageAdapter,
          task: !!options.taskStorageAdapter,
          workflow: !!options.workflowStorageAdapter,
          workflowExecution: !!options.workflowExecutionStorageAdapter,
          agentLoopCheckpoint: !!options.agentLoopCheckpointStorageAdapter,
        });
      }
    } catch (error) {
      logger.error("Failed to initialize storage adapters", { error });
      // Don't throw here - allow SDK to work without storage if needed
    }
    
    // Step 2: Preload lazy-loaded modules (TomlParserManager)
    try {
      await TomlParserManager.initialize();
    } catch (error) {
      logger.error(`Failed to initialize TOML parser: ${getErrorMessage(error)}`);
    }

    const presets = options?.presets;

    // Context compression is enabled by default, unless it is explicitly disabled.
    if (presets?.contextCompression?.enabled !== false) {
      try {
        const container = getContainer();
        const triggerRegistry = container.get(
          Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>,
        );
        const workflowRegistry = container.get(
          Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>,
        );

        registerContextCompression(
          triggerRegistry,
          workflowRegistry,
          presets?.contextCompression,
          false,
        );
      } catch (error) {
        logger.error(`Failed to bootstrap context compression preset: ${getErrorMessage(error)}`);
      }
    }

    // The predefined tools are enabled by default, unless they are explicitly disabled.
    if (presets?.predefinedTools?.enabled !== false) {
      try {
        const container = getContainer();
        const toolService = container.get(
          Identifiers.ToolRegistry as ServiceIdentifier<ToolRegistry>,
        );

        registerAllPredefinedContent(
          container.get(
            Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>,
          ),
          container.get(Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>),
          toolService,
          {
            contextCompression: { enabled: false }, // Already registered separately above.
            tools: {
              enabled: presets?.predefinedTools?.enabled,
              config: {
                allowList: presets?.predefinedTools?.allowList,
                blockList: presets?.predefinedTools?.blockList,
                config: presets?.predefinedTools?.config,
              },
            },
            skipIfExists: true,
          },
        );

        logger.info("Predefined tools registered successfully");
      } catch (error) {
        logger.error(`Failed to bootstrap predefined tools: ${getErrorMessage(error)}`);
      }
    }

    // The predefined prompt word templates are enabled by default, unless they are explicitly disabled.
    if (presets?.predefinedPrompts?.enabled !== false) {
      try {
        registerPredefinedPromptTemplates();
        logger.info("Predefined prompt templates registered successfully");
      } catch (error) {
        logger.error(`Failed to bootstrap predefined prompt templates: ${getErrorMessage(error)}`);
      }
    }

    // Subsequent registration for other L1/L2 content can be done automatically based on the selected options.

    // Mark as bootstrapped
    this.isBootstrapped = true;

    // Call complete hook if provided
    await options?.hooks?.onBootstrapComplete?.();
  }

  /**
   * Wait for SDK bootstrap to complete
   * Useful when apps need to ensure all initialization is done before proceeding
   * @returns Promise that resolves when bootstrap is complete
   */
  async waitForReady(): Promise<void> {
    if (this.bootstrapPromise) {
      await this.bootstrapPromise;
    }
  }

  /**
   * Check if SDK has completed bootstrap
   * @returns true if bootstrap is complete, false otherwise
   */
  isReady(): boolean {
    return this.isBootstrapped;
  }

  /**
   * Get the workflow API
   */
  get workflows() {
    return this.factory.createWorkflowAPI();
  }

  /**
   * Obtain the workflow execution API
   */
  get executions() {
    return this.factory.createWorkflowExecutionAPI();
  }

  /**
   * Get Node Template API
   */
  get nodeTemplates() {
    return this.factory.createNodeTemplateAPI();
  }

  /**
   * Get Trigger Template API
   */
  get triggerTemplates() {
    return this.factory.createTriggerTemplateAPI();
  }

  /**
   * Get the tool API
   */
  get tools() {
    return this.factory.createToolAPI();
  }

  /**
   * Get the script API
   */
  get scripts() {
    return this.factory.createScriptAPI();
  }

  /**
   * Get Profile API
   */
  get profiles() {
    return this.factory.createProfileAPI();
  }

  /**
   * Obtain User Interaction API
   */
  get userInteractions() {
    return this.factory.createUserInteractionAPI();
  }

  /**
   * Get the HumanRelay API
   */
  get humanRelay() {
    return this.factory.createHumanRelayAPI();
  }

  /**
   * Get the event API
   */
  get events() {
    return this.factory.createEventAPI();
  }

  /**
   * Obtain Trigger API
   */
  get triggers() {
    return this.factory.createTriggerAPI();
  }

  /**
   * Get variable API
   */
  get variables() {
    return this.factory.createVariableAPI();
  }

  /**
   * Get the message API
   */
  get messages() {
    return this.factory.createMessageAPI();
  }

  /**
   * Get the Skill API
   */
  get skills() {
    return this.factory.createSkillAPI();
  }

  /**
   * Obtain an instance of the API factory.
   */
  getFactory(): APIFactory {
    return this.factory;
  }

  /**
   * Reset the SDK
   */
  reset(): void {
    this.factory.reset();
  }

  /**
   * Check the health status of the SDK.
   */
  async healthCheck(): Promise<{
    status: "healthy" | "degraded" | "unhealthy";
    details: Record<string, unknown>;
  }> {
    const details: Record<string, unknown> = {};
    const modules = [
      { name: "workflows", check: async () => getData(await this.workflows.count()) },
      { name: "executions", check: async () => getData(await this.executions.count()) },
      { name: "tools", check: async () => getData(await this.tools.count()) },
      { name: "scripts", check: async () => getData(await this.scripts.count()) },
      { name: "nodeTemplates", check: async () => getData(await this.nodeTemplates.count()) },
      { name: "triggerTemplates", check: async () => getData(await this.triggerTemplates.count()) },
      { name: "profiles", check: async () => getData(await this.profiles.count()) },
      {
        name: "userInteractions",
        check: async () => getData(await this.userInteractions.getConfigCount()),
      },
      { name: "humanRelay", check: async () => getData(await this.humanRelay.getConfigCount()) },
    ];

    let overallStatus: "healthy" | "degraded" | "unhealthy" = "healthy";

    for (const module of modules) {
      try {
        await module.check();
        details[module.name] = { status: "healthy" };
      } catch (error) {
        details[module.name] = {
          status: "unhealthy",
          error: getErrorMessage(error),
        };
        overallStatus = "degraded";
      }
    }

    // Add storage adapter health checks
    try {
      const storageService = StorageInitializationService.getInstance();
      if (storageService.isInitialized()) {
        const storageHealth = await storageService.healthCheck();
        details["storageAdapters"] = {
          healthy: storageHealth.overallHealthy,
          results: storageHealth.results,
          timestamp: storageHealth.timestamp.toISOString(),
        };
        
        if (!storageHealth.overallHealthy) {
          overallStatus = "degraded";
        }
      } else {
        details["storageAdapters"] = { status: "not_initialized" };
      }
    } catch (error) {
      details["storageAdapters"] = {
        status: "unhealthy",
        error: getErrorMessage(error),
      };
      overallStatus = "degraded";
    }

    return { status: overallStatus, details };
  }

  /**
   * Terminate the SDK instance and clean up resources.
   */
  async destroy(): Promise<void> {
    // Call destroy hook if provided
    await this.pendingOptions?.hooks?.onDestroy?.();

    // Shutdown storage adapters first
    try {
      await this.shutdown();
    } catch (error) {
      logger.error("Error during shutdown", { error: getErrorMessage(error) });
    }

    // Clean up the resources of each module.
    const cleanupTasks = [
      { name: "workflows", task: () => this.workflows.clear() },
      { name: "executions", task: () => this.executions.clear() },
      { name: "tools", task: () => this.tools.clear() },
      { name: "scripts", task: () => this.scripts.clear() },
      { name: "nodeTemplates", task: () => this.nodeTemplates.clear() },
      { name: "triggerTemplates", task: () => this.triggerTemplates.clear() },
      { name: "profiles", task: () => this.profiles.clear() },
      { name: "userInteractions", task: () => this.userInteractions.clear() },
      { name: "humanRelay", task: () => this.humanRelay.clear() },
    ];

    for (const { name, task } of cleanupTasks) {
      try {
        await task();
      } catch (error) {
        logger.error(`Failed to cleanup ${name} resource`, { error: getErrorMessage(error) });
      }
    }

    // Clean up factory instances
    this.factory.reset();

    // Clean up the dependency manager
    try {
      // The DI container will automatically clean up all singleton services.
      const { resetContainer } = await import("../../../core/di/index.js");
      resetContainer();
    } catch (error) {
      logger.error("Failed to cleanup dependencies", { error: getErrorMessage(error) });
    }

    logger.info("SDK instance destroyed");
  }

  /**
   * Shutdown the SDK gracefully
   * Closes all storage adapters and releases resources
   */
  async shutdown(): Promise<void> {
    logger.info("Shutting down SDK...");

    try {
      // Shutdown storage adapters through StorageInitializationService
      const storageService = StorageInitializationService.getInstance();
      if (storageService.isInitialized()) {
        await storageService.shutdown();
        logger.info("Storage adapters shut down successfully");
      } else {
        logger.debug("StorageInitializationService not initialized, skipping shutdown");
      }
    } catch (error) {
      logger.error("Failed to shutdown storage adapters", { error: getErrorMessage(error) });
      // Continue with cleanup even if storage shutdown fails
    }

    logger.info("SDK shutdown completed");
  }
}

/**
 * Global SDK instance
 */
let globalSDK: SDK | null = null;

/**
 * Get the global SDK instance.
 * Delay initialization to avoid initializing the DI container when the module is loaded.
 * 
 * Usage Pattern:
 * ```typescript
 * // Simple usage with automatic initialization
 * const sdk = getSDK({ presets: { predefinedTools: { enabled: true } } });
 * 
 * // Explicit control - wait for initialization
 * const sdk = getSDK(options);
 * await sdk.waitForReady();
 * // Now safe to use all SDK features
 * 
 * // Check readiness status
 * if (sdk.isReady()) {
 *   // SDK is fully initialized
 * }
 * ```
 * 
 * @param options SDK configuration options (only effective during the initial initialization)
 * @returns Global SDK instance
 */
export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;
}

// Export dependency management class
export { APIDependencyManager };
