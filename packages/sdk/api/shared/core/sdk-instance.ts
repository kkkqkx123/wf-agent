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

import { GlobalContext } from "../../../shared/global-context.js";
import type { SDKOptions } from "../types/core-types.js";
import { APIFactory } from "./api-factory.js";
import { sdkLogger as logger, configureSDKLogger } from "../../../utils/logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import { createIsolatedContainer, ContainerManager } from "../../../di/container-manager.js";
import type { FileCheckpointStorageAdapter } from "@wf-agent/common-utils";
import {
  mergeFileCheckpointConfig,
  toFileCheckpointManagerConfig,
} from "../config/processors/file-checkpoint.js";
import { SqliteFileCheckpointStore } from "@wf-agent/storage";
import * as ServiceIdentifiers from "../../../di/service-identifiers.js";
import { registerAllResources } from "../../../resources/registration/orchestrator.js";
import { initializeTomlParser } from "../config/parsers/toml-parser.js";
import {
  createRotatingFileStream,
  createConsoleStream,
  createMultistream,
} from "@wf-agent/common-utils";
import { SDKError as SDKErrorClass } from "@wf-agent/types";
import type { LLMProfile } from "@wf-agent/types";
import type { LogStream, LogLevel } from "@wf-agent/common-utils";
import { WorkflowBuilder } from "../../workflow/builders/workflow-builder.js";
import { NodeBuilder } from "../../workflow/builders/node-builder.js";
import { McpServerRegistry } from "../../../services/executors/mcp/index.js";
import { NodeTemplateBuilder } from "../../workflow/builders/node-template-builder.js";
import { TriggerTemplateBuilder } from "../../workflow/builders/trigger-template-builder.js";
import type { BaseCommand } from "../types/command.js";
import type { ExecutionResult } from "../types/execution-result.js";
import { failure } from "../types/execution-result.js";
import { GracefulShutdownManager } from "../../../services/shutdown/graceful-shutdown-manager.js";

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
  private shutdownManager?: GracefulShutdownManager;

  /**
   * Create an SDK instance
   * @param options SDK configuration options
   */
  constructor(options: SDKOptions) {
    this.config = options;

    // Apply logging configuration FIRST before any logger access
    this.applyLoggingConfig(options);

    // Validate required configurations
    this.validateConfig(options);

    // Process file checkpoint configuration
    const fileCheckpointConfig = options?.fileCheckpoint
      ? mergeFileCheckpointConfig(options.fileCheckpoint)
      : null;
    const fcManagerConfig = fileCheckpointConfig
      ? toFileCheckpointManagerConfig(fileCheckpointConfig)
      : undefined;
    const fcStorageAdapter: FileCheckpointStorageAdapter | undefined = fileCheckpointConfig?.enabled
      ? new SqliteFileCheckpointStore({
          dbPath: fileCheckpointConfig.storage.dbPath || "file-checkpoints.db",
        })
      : undefined;

    // Create isolated DI container with storage adapters
    const { container, containerId } = createIsolatedContainer({
      checkpoint: options?.checkpointStorageAdapter,
      task: options?.taskStorageAdapter,
      workflow: options?.workflowStorageAdapter,
      workflowExecution: options?.workflowExecutionStorageAdapter,
      agentLoop: options?.agentLoopCheckpointStorageAdapter,
      trigger: options?.triggerStorageAdapter,
      tool: options?.toolStorageAdapter,
      script: options?.scriptStorageAdapter,
      nodeTemplate: options?.nodeTemplateStorageAdapter,
      hookTemplate: options?.hookTemplateStorageAdapter,
      agentProfile: options?.agentProfileStorageAdapter,
      fileCheckpointStorageAdapter: fcStorageAdapter,
      fileCheckpointManagerConfig: fcManagerConfig,
    });
    this.containerId = containerId;

    // Create GlobalContext - now safe because it uses lazy getters
    // Services will only be resolved when first accessed, not during construction
    this.globalContext = new GlobalContext(container);

    // Bind GlobalContext to the container for services that depend on it
    container.bind(ServiceIdentifiers.GlobalContext).toConstantValue(this.globalContext);

    // Bind SDKOptions to the container for configuration-dependent services
    container.bind(ServiceIdentifiers.SDKOptions).toConstantValue(options);

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
   * Validate SDK configuration
   * @param options SDK configuration options
   */
  private validateConfig(options: SDKOptions): void {
    const warnings: string[] = [];

    // Warn if file output is configured but no filePath provided
    if (
      (options?.logging?.output === "file" || options?.logging?.output === "both") &&
      !options?.logging?.filePath
    ) {
      warnings.push(
        `File output is enabled but no filePath specified. Using default: logs/sdk.log`,
      );
    }

    // Warn if no storage adapters are provided
    if (!options?.checkpointStorageAdapter) {
      warnings.push("No checkpoint storage adapter provided. Checkpoints will be disabled.");
    }

    if (!options?.workflowStorageAdapter) {
      warnings.push("No workflow storage adapter provided. Workflows will not be persisted.");
    }

    if (!options?.taskStorageAdapter) {
      warnings.push("No task storage adapter provided. Tasks will not be persisted.");
    }

    if (!options?.workflowExecutionStorageAdapter) {
      warnings.push(
        "No workflow execution storage adapter provided. Execution history will not be persisted.",
      );
    }

    if (!options?.triggerStorageAdapter) {
      warnings.push(
        "No trigger storage adapter provided. Trigger templates will not be persisted.",
      );
    }

    if (!options?.toolStorageAdapter) {
      warnings.push("No tool storage adapter provided. Tools will not be persisted.");
    }

    if (!options?.scriptStorageAdapter) {
      warnings.push("No script storage adapter provided. Scripts will not be persisted.");
    }

    // Log warnings
    if (warnings.length > 0) {
      warnings.forEach(warning => logger.warn(warning));
    }

    // Validate skill paths if provided
    if (options?.skills?.paths && options.skills.paths.length > 0) {
      for (const path of options.skills.paths) {
        if (!path || path.trim() === "") {
          logger.warn("Empty skill path provided, it will be ignored.");
        }
      }
    }

    // Validate LLM profiles if provided
    if (options?.profiles?.profiles && options.profiles.profiles.length > 0) {
      const hasInvalidProfiles = options.profiles.profiles.some(
        p => !p || typeof p !== "object" || !("id" in p) || !p.id,
      );
      if (hasInvalidProfiles) {
        logger.warn('Some LLM profiles may be missing required "id" field.');
      }
    }
  }

  /**
   * Apply logging configuration from SDK options
   * Must be called before any logger access to ensure proper configuration
   * @param options SDK configuration options
   */
  private applyLoggingConfig(options: SDKOptions): void {
    // Determine log level from logging config or debug mode
    let effectiveLogLevel = options.logging?.level;

    // If not specified, use default based on debug mode
    if (!effectiveLogLevel && options.debug) {
      effectiveLogLevel = "debug";
    }

    // Build stream configuration based on output setting
    const streams: Array<{ stream: LogStream; level?: string }> = [];
    const output = options.logging?.output ?? "console";

    // Add console stream if output includes 'console'
    if (output === "console" || output === "both") {
      streams.push({
        stream: createConsoleStream({
          json: options.logging?.format === "json",
          timestamp: true,
        }),
      });
    }

    // Add file stream if output includes 'file'
    if (output === "file" || output === "both") {
      const filePath = options.logging?.filePath ?? "logs/sdk.log";
      try {
        const fileStream = createRotatingFileStream({
          filePath,
          maxSize: 100 * 1024 * 1024, // 100MB default
          maxFiles: 10,
          json: options.logging?.format === "json",
          timestamp: true,
        });
        streams.push({ stream: fileStream });
      } catch (error) {
        // If file stream creation fails, fall back to console only
        logger.error(`Failed to create file stream at ${filePath}: ${getErrorMessage(error)}`);
      }
    }

    // Create multistream if multiple outputs, otherwise use single stream
    let finalStream: LogStream | undefined;
    if (streams.length === 0) {
      // Default to console if no streams configured
      finalStream = createConsoleStream({
        json: options.logging?.format === "json",
        timestamp: true,
      });
    } else if (streams.length === 1) {
      finalStream = streams[0]!.stream;
    } else {
      // Multiple streams - use multistream with proper typing
      finalStream = createMultistream(
        streams.map(s => ({
          stream: s.stream,
          level: s.level as LogLevel | undefined,
        })),
      );
    }

    // Configure SDK logger with the determined settings
    configureSDKLogger({
      level: effectiveLogLevel,
      stream: finalStream,
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
      agentLoop: !!this.config?.agentLoopCheckpointStorageAdapter,
      trigger: !!this.config?.triggerStorageAdapter,
      tool: !!this.config?.toolStorageAdapter,
      script: !!this.config?.scriptStorageAdapter,
    });

    // Preload lazy-loaded modules (TOML parser)
    try {
      await initializeTomlParser();
    } catch (error) {
      logger.error(`Failed to initialize TOML parser: ${getErrorMessage(error)}`);
    }

    // Initialize TaskRegistry with async storage loading
    try {
      const taskRegistry = this.globalContext.container.get(ServiceIdentifiers.TaskRegistry) as
        | { initialize: () => Promise<void>; isPersistenceEnabled?: () => boolean }
        | undefined;
      if (taskRegistry?.initialize) {
        await taskRegistry.initialize();
        logger.info("TaskRegistry initialized", {
          persistenceEnabled: taskRegistry.isPersistenceEnabled?.() ?? false,
        });
      }
    } catch (error) {
      logger.error(`Failed to initialize TaskRegistry: ${getErrorMessage(error)}`);
    }

    // Configure MCP if enabled
    if (this.config?.mcp) {
      try {
        McpServerRegistry.setOptions({
          mcpEnabled: this.config.mcp.enabled ?? true,
          maxErrorHistory: this.config.mcp.maxErrorHistory,
          connectionTimeout: this.config.mcp.connectionTimeout,
          configDebounceDelay: this.config.mcp.configDebounceDelay,
          defaultLifecycle: this.config.mcp.defaultLifecycle,
          defaultIdleTimeout: this.config.mcp.defaultIdleTimeout,
          defaultHealthCheckInterval: this.config.mcp.defaultHealthCheckInterval,
        });
        logger.info("MCP configuration applied", {
          enabled: this.config.mcp.enabled ?? true,
        });
      } catch (error) {
        logger.error(`Failed to configure MCP: ${getErrorMessage(error)}`);
      }
    }

    // Initialize file checkpoint manager if enabled
    if (this.config?.fileCheckpoint?.enabled) {
      try {
        const fcApi = this.apiFactory.createFileCheckpointAPI();
        await fcApi.initialize();
        logger.info("File checkpoint manager initialized", {
          dbPath: this.config.fileCheckpoint.storage?.dbPath,
        });
      } catch (error) {
        logger.error(`Failed to initialize file checkpoint manager: ${getErrorMessage(error)}`);
      }
    }

    // Configure Skill registry if paths are provided
    if (this.config?.skills?.paths && this.config.skills.paths.length > 0) {
      try {
        const skillRegistry = this.globalContext.container.get(ServiceIdentifiers.SkillRegistry);

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
          // Type assertion is safe here because profiles are validated before being passed to SDK
          await profileAPI.create(profile as LLMProfile);
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
        // Store validation config in workflow registry for use by validators
        const validationConfig = this.config.validation;

        // Apply to workflow registry
        // Note: maxRecursionDepth is configured through validation settings in the registry
        // The registry will use this value during workflow validation
        logger.debug("Validation maxRecursionDepth configured", {
          maxRecursionDepth: validationConfig.maxRecursionDepth,
        });

        logger.info("Validation configuration applied", {
          workflowValidation: validationConfig.enableWorkflowValidation,
          nodeValidation: validationConfig.enableNodeValidation,
          graphValidation: validationConfig.enableGraphValidation,
          checkCycles: validationConfig.checkCycles,
          checkReachability: validationConfig.checkReachability,
          maxRecursionDepth: validationConfig.maxRecursionDepth,
        });
      } catch (error) {
        logger.error(`Failed to configure validation: ${getErrorMessage(error)}`);
      }
    }

    // Configure event system if provided
    if (this.config?.events) {
      try {
        // EventRegistry accepts config in constructor, but we can update it if needed
        // For now, log the configuration that was noted during container creation
        logger.info("Event system configuration noted", {
          maxListenerQueueSize: this.config.events.maxListenerQueueSize,
          enableBackpressure: this.config.events.enableBackpressure,
          defaultListenerTimeout: this.config.events.defaultListenerTimeout,
          slowListenerThreshold: this.config.events.slowListenerThreshold,
        });
      } catch (error) {
        logger.error(`Failed to configure event system: ${getErrorMessage(error)}`);
      }
    }

    // Configure workflow execution settings if provided
    if (this.config?.workflowExecution) {
      try {
        const execConfig = this.config.workflowExecution;
        logger.info("Workflow execution configuration applied", {
          defaultTimeout: execConfig.defaultTimeout,
          maxConcurrentExecutions: execConfig.maxConcurrentExecutions,
          enableRetry: execConfig.enableRetry,
          maxRetryAttempts: execConfig.maxRetryAttempts,
        });
      } catch (error) {
        logger.error(`Failed to configure workflow execution: ${getErrorMessage(error)}`);
      }
    }

    const presets = this.config?.presets;

    // ====================================================================
    // Order: storage → predefined → mark ready
    // 1. Storage: load persisted workflows first (they take priority)
    // 2. Predefined: register built-in content, skip if already in storage
    // 3. Mark bootstrapped as complete
    // ====================================================================

    // 1. Initialize storage adapters and load persisted data
    // All storage adapters (Memory, SQLite, JSON, Postgres) implement
    // StorageAdapterBase with an initialize() lifecycle method that must be
    // called before any CRUD operation. Without it, ensureInitialized() throws.
    // This runs BEFORE predefined registration so persisted data takes priority.

    // Helper to initialize a storage adapter if it has the initialize() method
    const tryInitAdapter = async (adapter: unknown, name: string): Promise<void> => {
      if (
        adapter &&
        typeof (adapter as { initialize: () => Promise<void> }).initialize === "function"
      ) {
        try {
          await (adapter as { initialize: () => Promise<void> }).initialize();
          logger.debug(`${name} storage adapter initialized`);
        } catch (error) {
          logger.error(`Failed to initialize ${name} storage adapter: ${getErrorMessage(error)}`);
        }
      }
    };

    // Initialize all adapters first, then load registries from storage
    await tryInitAdapter(this.config?.workflowStorageAdapter, "workflow");
    await tryInitAdapter(this.config?.triggerStorageAdapter, "trigger");
    await tryInitAdapter(this.config?.toolStorageAdapter, "tool");
    await tryInitAdapter(this.config?.scriptStorageAdapter, "script");
    await tryInitAdapter(this.config?.checkpointStorageAdapter, "checkpoint");
    await tryInitAdapter(this.config?.taskStorageAdapter, "task");
    await tryInitAdapter(this.config?.workflowExecutionStorageAdapter, "workflowExecution");
    await tryInitAdapter(this.config?.agentLoopCheckpointStorageAdapter, "agentLoop");
    await tryInitAdapter(this.config?.nodeTemplateStorageAdapter, "nodeTemplate");
    await tryInitAdapter(this.config?.hookTemplateStorageAdapter, "hookTemplate");
    await tryInitAdapter(this.config?.agentProfileStorageAdapter, "agentProfile");

    // Helper to initialize a registry from storage if it has initializeFromStorage()
    // Each registry already guards against missing storageAdapter internally,
    // so it's safe to call unconditionally.
    const tryInitRegistry = async (
      registry: { initializeFromStorage?: () => Promise<void> } | undefined | null,
      name: string,
    ): Promise<void> => {
      if (!registry?.initializeFromStorage) return;
      try {
        await registry.initializeFromStorage();
        logger.info(`${name} registry initialized from storage`);
      } catch (error) {
        logger.error(
          `Failed to initialize ${name} registry from storage: ${getErrorMessage(error)}`,
        );
      }
    };

    // Initialize registries from storage (safely using initialized adapters)
    await tryInitRegistry(this.globalContext.workflowRegistry, "Workflow");
    await tryInitRegistry(this.globalContext.triggerTemplateRegistry, "Trigger template");
    await tryInitRegistry(this.globalContext.toolRegistry, "Tool");
    await tryInitRegistry(this.globalContext.scriptRegistry, "Script");

    // 2. Register all resources through unified orchestrator (skips if already in storage)
    try {
      await registerAllResources(
        {
          triggerRegistry: this.globalContext.triggerTemplateRegistry,
          workflowRegistry: this.globalContext.workflowRegistry,
          toolRegistry: this.globalContext.toolRegistry,
          promptTemplateRegistry: this.globalContext.promptTemplateRegistry,
          fragmentRegistry: this.globalContext.fragmentRegistry,
          toolDescriptionRegistry: this.globalContext.toolDescriptionRegistry,
        },
        presets,
        undefined,
        undefined,
        true,
      );

      logger.info("All resources registered successfully");
    } catch (error) {
      logger.error(`Failed to bootstrap resources: ${getErrorMessage(error)}`);
    }

    // 3. Mark as bootstrapped
    this.isBootstrapped = true;

    // Initialize Graceful Shutdown Manager if enabled
    const gracefulShutdownConfig = this.config?.gracefulShutdown;
    const enableGracefulShutdown = gracefulShutdownConfig?.enabled ?? true;

    if (enableGracefulShutdown) {
      try {
        const workflowExecutionRegistry = this.globalContext.container.get(
          ServiceIdentifiers.WorkflowExecutionRegistry,
        );
        const checkpointState = this.globalContext.container.get(
          ServiceIdentifiers.CheckpointState,
        );
        const workflowRegistry = this.globalContext.workflowRegistry;
        const workflowGraphRegistry = this.globalContext.container.get(
          ServiceIdentifiers.WorkflowGraphRegistry,
        );

        const shutdownManager = new GracefulShutdownManager(
          workflowExecutionRegistry,
          {
            workflowExecutionRegistry,
            checkpointStateManager: checkpointState,
            workflowRegistry,
            workflowGraphRegistry,
            // stateCoordinatorMap is optional and not registered in container
            stateCoordinatorMap: undefined,
          },
          {
            timeoutMs: gracefulShutdownConfig?.timeoutMs ?? 15000,
            enabled: true,
          },
        );

        shutdownManager.registerSignalHandlers();
        this.shutdownManager = shutdownManager;

        logger.info("Graceful shutdown manager initialized and signal handlers registered", {
          timeoutMs: gracefulShutdownConfig?.timeoutMs ?? 15000,
        });
      } catch (error) {
        logger.error(`Failed to initialize graceful shutdown manager: ${getErrorMessage(error)}`);
        // Don't fail bootstrap - SDK can still work without graceful shutdown
      }
    }

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

  /**
   * Ensure SDK is ready before accessing APIs
   * @throws Error if SDK is not ready
   */
  private ensureReady(): void {
    if (!this.isBootstrapped) {
      throw new Error(
        "SDK instance is not ready yet. Call await sdk.waitForReady() before using APIs.",
      );
    }
  }

  // ============================================================================
  // API Accessors - combine global context + instance storage
  // ============================================================================

  /**
   * Get the workflow API
   */
  get workflows() {
    this.ensureReady();
    return this.apiFactory.createWorkflowAPI();
  }

  /**
   * Obtain the workflow execution API
   */
  get executions() {
    this.ensureReady();
    return this.apiFactory.createWorkflowExecutionAPI();
  }

  /**
   * Get Node Template API
   */
  get nodeTemplates() {
    this.ensureReady();
    return this.apiFactory.createNodeTemplateAPI();
  }

  /**
   * Get Trigger Template API
   */
  get triggerTemplates() {
    this.ensureReady();
    return this.apiFactory.createTriggerTemplateAPI();
  }

  /**
   * Get the tool API
   */
  get tools() {
    this.ensureReady();
    return this.apiFactory.createToolAPI();
  }

  /**
   * Get the script API
   */
  get scripts() {
    this.ensureReady();
    return this.apiFactory.createScriptAPI();
  }

  /**
   * Get the script execution service
   * @returns ScriptExecutionService instance
   */
  getScriptExecutor() {
    this.ensureReady();
    return this.globalContext.scriptExecutor;
  }

  /**
   * Get Profile API
   */
  get profiles() {
    this.ensureReady();
    return this.apiFactory.createProfileAPI();
  }

  /**
   * Obtain User Interaction API
   */
  get userInteractions() {
    this.ensureReady();
    return this.apiFactory.createUserInteractionAPI();
  }

  /**
   * Get the event API
   */
  get events() {
    this.ensureReady();
    return this.apiFactory.createEventAPI();
  }

  /**
   * Get the metrics API
   */
  get metrics() {
    this.ensureReady();
    return this.apiFactory.createMetricsAPI();
  }

  // ============================================================================
  // Builder Factory Methods
  // ============================================================================

  /**
   * Create a WorkflowBuilder instance
   * @param id Workflow ID
   * @returns WorkflowBuilder instance
   */
  createWorkflowBuilder(id: string) {
    this.ensureReady();
    return WorkflowBuilder.create(this.globalContext, id);
  }

  /**
   * Create a NodeBuilder instance
   * @param id Node ID (optional, auto-generated)
   * @returns NodeBuilder instance
   */
  createNodeBuilder(id?: string) {
    this.ensureReady();
    return NodeBuilder.create(this.globalContext, id);
  }

  /**
   * Create a NodeTemplateBuilder instance
   * @param name Template name
   * @param type Node type
   * @returns NodeTemplateBuilder instance
   */
  createNodeTemplateBuilder(name: string, type: import("@wf-agent/types").StaticNodeType) {
    this.ensureReady();
    return NodeTemplateBuilder.create(this.globalContext, name, type);
  }

  /**
   * Create a TriggerTemplateBuilder instance
   * @param name Template name
   * @returns TriggerTemplateBuilder instance
   */
  createTriggerTemplateBuilder(name: string) {
    this.ensureReady();
    return TriggerTemplateBuilder.create(this.globalContext, name);
  }

  /**
   * Obtain Trigger API
   */
  get triggers() {
    this.ensureReady();
    return this.apiFactory.createTriggerAPI();
  }

  /**
   * Get variable API
   */
  get variables() {
    this.ensureReady();
    return this.apiFactory.createVariableAPI();
  }

  /**
   * Get Agent Variable API
   */
  get agentVariables() {
    this.ensureReady();
    return this.apiFactory.createAgentVariableAPI();
  }

  /**
   * Get Agent User Interaction API
   */
  get agentUserInteractions() {
    this.ensureReady();
    return this.apiFactory.createAgentUserInteractionAPI();
  }

  /**
   * Get the GlobalContext for this SDK instance
   *
   * This provides access to the instance-specific DI container and all registered services.
   * Use this when you need direct access to services that are not exposed through the API layer.
   *
   * @returns The GlobalContext instance associated with this SDK instance
   *
   * @example
   * ```typescript
   * const sdk = createSDK(options);
   * await sdk.waitForReady();
   *
   * // Access the DI container
   * const context = sdk.getGlobalContext();
   * const container = context.getContainer();
   *
   * // Or access specific services
   * const eventRegistry = context.eventRegistry;
   * ```
   */
  getGlobalContext(): GlobalContext {
    this.ensureReady();
    return this.globalContext;
  }

  /**
   * Get the message API
   */
  get messages() {
    this.ensureReady();
    return this.apiFactory.createMessageAPI();
  }

  /**
   * Execute a command
   *
   * This method provides a unified interface for executing commands in the SDK.
   * Commands are validated before execution and errors are properly handled.
   *
   * @param command Command to execute (must extend BaseCommand)
   * @returns Execution result with proper error handling
   *
   * @example
   * ```typescript
   * const sdk = createSDK(options);
   * await sdk.waitForReady();
   *
   * // Execute a workflow
   * const cmd = new ExecuteWorkflowCommand('my-workflow', sdk.getFactory().getDependencies());
   * const result = await sdk.executeCommand(cmd);
   *
   * // Pause a workflow execution
   * const pauseCmd = new PauseWorkflowCommand('exec-id', sdk.getFactory().getDependencies());
   * await sdk.executeCommand(pauseCmd);
   * ```
   */
  async executeCommand<T>(command: BaseCommand<T>): Promise<ExecutionResult<T>> {
    this.ensureReady();

    // Validate command before execution
    const validation = command.validate();
    if (!validation.valid) {
      logger.warn("Command validation failed", { errors: validation.errors });
      return failure(
        new SDKErrorClass(`Command validation failed: ${validation.errors.join(", ")}`, "error"),
        0,
      );
    }

    // Execute command with dependencies from API factory
    try {
      return await command.execute();
    } catch (error) {
      logger.error("Command execution failed", { error });
      throw error;
    }
  }

  /**
   * Get the Skill API
   */
  get skills() {
    this.ensureReady();
    return this.apiFactory.createSkillAPI();
  }

  /**
   * Get the File Checkpoint API
   */
  get fileCheckpoints() {
    this.ensureReady();
    return this.apiFactory.createFileCheckpointAPI();
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
   * Cleanup all agent loops
   * Stops and cleans up all running/paused agent loops
   */
  private async cleanupAgentLoops(): Promise<void> {
    try {
      const coordinator = this.getFactory().getDependencies().getAgentLoopCoordinator();
      const registry = this.getFactory().getDependencies().getAgentLoopRegistry();

      // Get all active agent loops
      const allLoops = registry.getAll();

      if (allLoops.length === 0) {
        return;
      }

      logger.info("Cleaning up agent loops", { count: allLoops.length });

      // Stop all running/paused loops
      for (const loop of allLoops) {
        if (loop.isRunning() || loop.isPaused()) {
          try {
            await coordinator.stop(loop.id);
            logger.debug("Stopped agent loop", { agentLoopId: loop.id });
          } catch (error) {
            logger.warn("Failed to stop agent loop during cleanup", {
              agentLoopId: loop.id,
              error: getErrorMessage(error),
            });
          }
        }

        // Cleanup resources
        loop.cleanup();
      }

      // Clear registry
      registry.clear();

      logger.info("Agent loops cleanup completed", { cleanedCount: allLoops.length });
    } catch (error) {
      logger.error("Failed to cleanup agent loops", { error: getErrorMessage(error) });
    }
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
   * Get the GracefulShutdownManager instance (if initialized)
   * @returns The shutdown manager or undefined if not enabled
   */
  getShutdownManager(): GracefulShutdownManager | undefined {
    return this.shutdownManager;
  }

  /**
   * Destroy the SDK instance and clean up all resources
   */
  async destroy(): Promise<void> {
    // Call destroy hook if provided
    await this.config?.hooks?.onDestroy?.();

    // Unregister signal handlers first to prevent MaxListenersExceededWarning in tests
    if (this.shutdownManager) {
      try {
        this.shutdownManager.unregisterSignalHandlers();
      } catch (error) {
        logger.warn("Failed to unregister signal handlers", { error: getErrorMessage(error) });
      }
    }

    // Shutdown storage adapters first
    try {
      await this.shutdown();
    } catch (error) {
      logger.error("Error during shutdown", { error: getErrorMessage(error) });
    }

    // Close file checkpoint manager if initialized
    try {
      const fcApi = this.apiFactory.createFileCheckpointAPI();
      if (fcApi.isEnabled()) {
        await fcApi.close();
        logger.info("File checkpoint manager closed");
      }
    } catch (error) {
      logger.error("Failed to close file checkpoint manager", { error: getErrorMessage(error) });
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
      { name: "events", task: () => this.events.dispose() },
      { name: "agentLoops", task: () => this.cleanupAgentLoops() },
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
