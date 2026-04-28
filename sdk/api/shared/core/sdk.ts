/**
 * SDK Main Class
 * Provides a unified API entry point that integrates all functional modules.
 *
 * Reconstruction Notes:
 * - Use APIFactory to centrally manage the creation of API instances.
 * - Simplify the configuration process and remove unnecessary configuration mechanisms.
 * - Support the application layer to implement the CheckpointStorageCallback interface.
 */

import { APIFactory } from "./api-factory.js";
import { APIDependencyManager } from "./sdk-dependencies.js";
import { getData } from "../types/execution-result.js";
import type { SDKOptions } from "../types/core-types.js";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import {
  setStorageCallback,
  setWorkflowStorageCallback,
  setThreadStorageCallback,
  getContainer,
} from "../../../core/di/container-config.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { registerContextCompression } from "../../../resources/predefined/index.js";
import { registerPredefinedPromptTemplates } from "../../../resources/predefined/prompts/index.js";
import { registerAllPredefinedContent } from "../../../resources/predefined/registration.js";
import { TomlParserManager } from "../../../utils/toml-parser-manager.js";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type { TriggerTemplateRegistry } from "../../../core/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "../../../graph/stores/workflow-registry.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";

/**
 * SDK Main Class - Unified API Entry Point (Internal class, not exported)
 */
class SDK {
  private factory: APIFactory;
  private dependencies: APIDependencyManager;

  /**
   * Create an SDK instance
   * @param options SDK configuration options
   */
  constructor(options?: SDKOptions) {
    // If storage callbacks are provided, set them to the DI container.
    if (options?.checkpointStorageCallback) {
      setStorageCallback(options.checkpointStorageCallback);
    }
    if (options?.workflowStorageCallback) {
      setWorkflowStorageCallback(options.workflowStorageCallback);
    }
    if (options?.threadStorageCallback) {
      setThreadStorageCallback(options.threadStorageCallback);
    }

    // Initialize the API factory.
    this.factory = APIFactory.getInstance();
    // Initialize the dependency manager
    this.dependencies = new APIDependencyManager();

    // Initialize preload content (executed asynchronously, without blocking the constructor)
    this.bootstrap(options).catch(error => {
      logger.error(`Failed to bootstrap SDK: ${getErrorMessage(error)}`);
    });
  }

  /**
   * Initialize preset functionality
   * @param options SDK configuration options
   */
  private async bootstrap(options?: SDKOptions): Promise<void> {
    // Preload lazy-loaded modules (TomlParserManager)
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
  }

  /**
   * Get the workflow API
   */
  get workflows() {
    return this.factory.createWorkflowAPI();
  }

  /**
   * Obtain the thread API
   */
  get threads() {
    return this.factory.createThreadAPI();
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
      { name: "threads", check: async () => getData(await this.threads.count()) },
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

    return { status: overallStatus, details };
  }

  /**
   * Terminate the SDK instance and clean up resources.
   */
  async destroy(): Promise<void> {
    // Clean up the resources of each module.
    const cleanupTasks = [
      { name: "workflows", task: () => this.workflows.clear() },
      { name: "threads", task: () => this.threads.clear() },
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
}

/**
 * Global SDK instance
 */
let globalSDK: SDK | null = null;

/**
 * Get the global SDK instance.
 * Delay initialization to avoid initializing the DI container when the module is loaded.
 * @param options SDK configuration options (only effective during the initial initialization)
 */
export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;
}

// Export dependency management class
export { APIDependencyManager };
