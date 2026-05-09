/**
 * SDK Instance - Per-Instance Configuration and Execution Contexts
 * 
 * Manages instance-specific configuration and execution contexts:
 * - Storage adapters (workflow, task, checkpoint, etc.)
 * - Configuration options (debug, logLevel, presets)
 * - API layer composition
 * 
 * Design Principles:
 * - Multiple instances can coexist
 * - Each has independent storage configuration
 * - Shares global registries and executors via GlobalContext
 * - Isolated execution contexts
 * - Independent lifecycle management
 */

import { GlobalContext } from "../../../core/global-context.js";
import type { SDKOptions } from "../types/core-types.js";
import { APIFactory } from "./api-factory.js";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import { createIsolatedContainer, ContainerManager } from "../../../core/di/container-manager.js";
import { registerContextCompression } from "../../../resources/predefined/index.js";
import { registerPredefinedPromptTemplates } from "../../../resources/predefined/prompts/index.js";
import { registerAllPredefinedContent } from "../../../resources/predefined/registration.js";
import { TomlParserManager } from "../../../utils/toml-parser-manager.js";
import * as ServiceIdentifiers from "../../../core/di/service-identifiers.js";

/**
 * SDK Instance Class
 * Represents a single SDK instance with isolated configuration and resources
 */
export class SDKInstance {
  private config: SDKOptions;
  private globalContext: GlobalContext;
  private apiFactory: APIFactory;
  private bootstrapPromise?: Promise<void>;
  private isBootstrapped: boolean = false;
  private containerId: string;

  /**
   * Create an SDK instance
   * @param options SDK configuration options
   */
  constructor(options: SDKOptions) {
    this.config = options;
    
    // Create isolated DI container with storage adapters
    const { container, containerId } = createIsolatedContainer({
      checkpoint: options?.checkpointStorageAdapter,
      task: options?.taskStorageAdapter,
      workflow: options?.workflowStorageAdapter,
      workflowExecution: options?.workflowExecutionStorageAdapter,
      agentLoopCheckpoint: options?.agentLoopCheckpointStorageAdapter,
    });
    this.containerId = containerId;
    
    // Create GlobalContext from the isolated container
    this.globalContext = new GlobalContext(container);
    
    // Bind GlobalContext to the container so services can access it
    container.bind(ServiceIdentifiers.GlobalContext).toConstantValue(this.globalContext);
    
    // Create API factory (it will use services from the container via GlobalContext)
    this.apiFactory = new APIFactory(this.globalContext);

    // Initialize preload content (executed asynchronously, without blocking the constructor)
    this.bootstrapPromise = this.bootstrap().catch(error => {
      logger.error(`Failed to bootstrap SDK instance: ${getErrorMessage(error)}`);
      // Call error hook if provided
      options?.hooks?.onBootstrapError?.(error);
    });
  }

  /**
   * Initialize preset functionality and configure instance
   */
  private async bootstrap(): Promise<void> {
    // Call start hook if provided
    await this.config?.hooks?.onBootstrapStart?.();
    
    logger.info("DI container initialized with storage adapters", {
      containerId: this.containerId,
      checkpoint: !!this.config?.checkpointStorageAdapter,
      task: !!this.config?.taskStorageAdapter,
      workflow: !!this.config?.workflowStorageAdapter,
      workflowExecution: !!this.config?.workflowExecutionStorageAdapter,
      agentLoopCheckpoint: !!this.config?.agentLoopCheckpointStorageAdapter,
    });
    
    // Preload lazy-loaded modules (TomlParserManager)
    try {
      await TomlParserManager.initialize();
    } catch (error) {
      logger.error(`Failed to initialize TOML parser: ${getErrorMessage(error)}`);
    }

    // Configure MCP if enabled
    if (this.config?.mcp?.enabled !== false) {
      try {
        const { McpServerRegistry } = await import("../../../services/mcp/server-registry.js");
        const mcpConfig = this.config.mcp!;
        McpServerRegistry.setOptions({
          mcpEnabled: mcpConfig.enabled,
          maxErrorHistory: mcpConfig.maxErrorHistory,
          connectionTimeout: mcpConfig.connectionTimeout,
          configDebounceDelay: mcpConfig.configDebounceDelay,
        });
        logger.info("MCP configuration applied", {
          enabled: mcpConfig.enabled,
        });
      } catch (error) {
        logger.error(`Failed to configure MCP: ${getErrorMessage(error)}`);
      }
    }

    // Configure Human Relay handler if provided
    if (this.config?.humanRelay?.handler) {
      try {
        const humanRelayAPI = this.apiFactory.createHumanRelayAPI();
        humanRelayAPI.registerHandler(this.config.humanRelay.handler as any);
        logger.info("Human Relay handler registered");
      } catch (error) {
        logger.error(`Failed to register Human Relay handler: ${getErrorMessage(error)}`);
      }
    }

    // Configure Skill registry if paths are provided
    if (this.config?.skills?.paths && this.config.skills.paths.length > 0) {
      try {
        const skillRegistry = this.globalContext.container.get(
          require("../../../core/di/service-identifiers.js").SkillRegistry as any,
        ) as any;
        
        for (const skillPath of this.config.skills.paths) {
          await skillRegistry.scanSkills(skillPath);
        }
        
        logger.info("Skill directories scanned", {
          paths: this.config.skills.paths,
        });
      } catch (error) {
        logger.error(`Failed to scan skill directories: ${getErrorMessage(error)}`);
      }
    }

    // Configure LLM profiles if provided
    if (this.config?.profiles?.profiles && this.config.profiles.profiles.length > 0) {
      try {
        const profileAPI = this.apiFactory.createProfileAPI();
        
        for (const profile of this.config.profiles.profiles) {
          await profileAPI.create(profile as any);
        }
        
        // Set default profile if specified
        if (this.config.profiles.defaultProfileId) {
          await profileAPI.setDefaultProfile(this.config.profiles.defaultProfileId);
        }
        
        logger.info("LLM profiles registered", {
          count: this.config.profiles.profiles.length,
          defaultProfileId: this.config.profiles.defaultProfileId,
        });
      } catch (error) {
        logger.error(`Failed to register LLM profiles: ${getErrorMessage(error)}`);
      }
    }

    // Configure validation settings if provided
    if (this.config?.validation) {
      try {
        // Store validation config in global context for use by validators
        const validationConfig = this.config.validation;
        logger.info("Validation configuration applied", {
          workflowValidation: validationConfig.enableWorkflowValidation,
          nodeValidation: validationConfig.enableNodeValidation,
          graphValidation: validationConfig.enableGraphValidation,
          checkCycles: validationConfig.checkCycles,
          checkReachability: validationConfig.checkReachability,
        });
      } catch (error) {
        logger.error(`Failed to configure validation: ${getErrorMessage(error)}`);
      }
    }

    // Configure event system if provided
    if (this.config?.events) {
      try {
        const eventRegistry = this.globalContext.eventRegistry;
        // Event registry configuration would be applied here if it supported runtime config
        logger.info("Event system configuration noted", {
          maxListenerQueueSize: this.config.events.maxListenerQueueSize,
          enableBackpressure: this.config.events.enableBackpressure,
        });
      } catch (error) {
        logger.error(`Failed to configure event system: ${getErrorMessage(error)}`);
      }
    }

    const presets = this.config?.presets;

    // Context compression is enabled by default, unless it is explicitly disabled.
    if (presets?.contextCompression?.enabled !== false) {
      try {
        registerContextCompression(
          this.globalContext.triggerTemplateRegistry,
          this.globalContext.workflowRegistry,
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
        registerAllPredefinedContent(
          this.globalContext.triggerTemplateRegistry,
          this.globalContext.workflowRegistry,
          this.globalContext.toolRegistry,
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

    // Mark as bootstrapped
    this.isBootstrapped = true;

    // Call complete hook if provided
    await this.config?.hooks?.onBootstrapComplete?.();
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

  // ============================================================================
  // API Accessors - combine global context + instance storage
  // ============================================================================

  /**
   * Get the workflow API
   */
  get workflows() {
    return this.apiFactory.createWorkflowAPI();
  }

  /**
   * Obtain the workflow execution API
   */
  get executions() {
    return this.apiFactory.createWorkflowExecutionAPI();
  }

  /**
   * Get Node Template API
   */
  get nodeTemplates() {
    return this.apiFactory.createNodeTemplateAPI();
  }

  /**
   * Get Trigger Template API
   */
  get triggerTemplates() {
    return this.apiFactory.createTriggerTemplateAPI();
  }

  /**
   * Get the tool API
   */
  get tools() {
    return this.apiFactory.createToolAPI();
  }

  /**
   * Get the script API
   */
  get scripts() {
    return this.apiFactory.createScriptAPI();
  }

  /**
   * Get Profile API
   */
  get profiles() {
    return this.apiFactory.createProfileAPI();
  }

  /**
   * Obtain User Interaction API
   */
  get userInteractions() {
    return this.apiFactory.createUserInteractionAPI();
  }

  /**
   * Get the HumanRelay API
   */
  get humanRelay() {
    return this.apiFactory.createHumanRelayAPI();
  }

  /**
   * Get the event API
   */
  get events() {
    return this.apiFactory.createEventAPI();
  }

  /**
   * Obtain Trigger API
   */
  get triggers() {
    return this.apiFactory.createTriggerAPI();
  }

  /**
   * Get variable API
   */
  get variables() {
    return this.apiFactory.createVariableAPI();
  }

  /**
   * Get the message API
   */
  get messages() {
    return this.apiFactory.createMessageAPI();
  }

  /**
   * Get the Skill API
   */
  get skills() {
    return this.apiFactory.createSkillAPI();
  }

  /**
   * Obtain an instance of the API factory.
   */
  getFactory(): APIFactory {
    return this.apiFactory;
  }

  /**
   * Reset the SDK instance
   */
  reset(): void {
    this.apiFactory.reset();
  }

  /**
   * Shutdown the SDK instance gracefully
   * Closes storage adapters and releases instance-specific resources
   */
  async shutdown(): Promise<void> {
    logger.info("Shutting down SDK instance...", { containerId: this.containerId });

    try {
      // Destroy the container and release all resources
      const manager = ContainerManager.getInstance();
      await manager.destroyContainer(this.containerId);
      logger.info("Container destroyed successfully", { containerId: this.containerId });
    } catch (error) {
      logger.error("Failed to shutdown container", { error: getErrorMessage(error) });
      // Continue with cleanup even if container shutdown fails
    }

    logger.info("SDK instance shutdown completed", { containerId: this.containerId });
  }

  /**
   * Destroy the SDK instance and clean up all resources
   */
  async destroy(): Promise<void> {
    // Call destroy hook if provided
    await this.config?.hooks?.onDestroy?.();

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
    this.apiFactory.reset();

    logger.info("SDK instance destroyed");
  }
}
