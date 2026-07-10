/**
 * CLI Dependency Container
 * 
 * Provides centralized dependency management for the CLI application.
 * Eliminates global state and enables better testability.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import type { CLIConfig } from "../config/index.js";
import { TerminalManager } from "../services/terminal/terminal-manager.js";
import { ExecutionService } from "../services/execution/execution-service.js";
import { WorkflowExecutionAdapter } from "../adapters/workflow-execution-adapter.js";
import type { CLIUserInteractionManager } from "../handlers/user-interaction/index.js";

/**
 * CLI Dependency Container
 * Manages all major services and their dependencies
 */
export class CLIDependencyContainer {
  private sdk: SDKInstance;
  private terminalManager: TerminalManager;
  private executionService: ExecutionService;
  private workflowExecutionAdapter: WorkflowExecutionAdapter;
  private interactionHandler: CLIUserInteractionManager | null = null;

  constructor(sdk: SDKInstance, _config: CLIConfig) {
    this.sdk = sdk;
    this.terminalManager = new TerminalManager();
    this.executionService = new ExecutionService(this.sdk, this.terminalManager);
    this.workflowExecutionAdapter = new WorkflowExecutionAdapter();
  }

  /**
   * Register the user interaction handler for lifecycle management
   */
  registerInteractionHandler(handler: CLIUserInteractionManager): void {
    this.interactionHandler = handler;
  }

  /**
   * Get the registered interaction handler
   */
  getInteractionHandler(): CLIUserInteractionManager | null {
    return this.interactionHandler;
  }

  /**
   * Get the SDK instance
   */
  getSDK(): SDKInstance {
    return this.sdk;
  }

  /**
   * Get the Terminal Manager
   */
  getTerminalManager(): TerminalManager {
    return this.terminalManager;
  }

  /**
   * Get the Execution Service
   */
  getExecutionService(): ExecutionService {
    return this.executionService;
  }

  /**
   * Get the Workflow Execution Adapter
   */
  getWorkflowExecutionAdapter(): WorkflowExecutionAdapter {
    return this.workflowExecutionAdapter;
  }

  /**
   * Cleanup all resources
   */
  async cleanup(): Promise<void> {
    await this.executionService.cleanup();
    if (this.interactionHandler) {
      this.interactionHandler.cleanup();
    }
  }
}

/**
 * Global container instance (for backward compatibility during migration)
 * TODO: Remove this in Phase 4 when all commands use dependency injection
 */
let globalContainer: CLIDependencyContainer | null = null;

/**
 * Initialize the global container
 */
export function initializeContainer(sdk: SDKInstance, config: CLIConfig): CLIDependencyContainer {
  globalContainer = new CLIDependencyContainer(sdk, config);
  return globalContainer;
}

/**
 * Get the global container instance
 */
export function getContainer(): CLIDependencyContainer {
  if (!globalContainer) {
    throw new Error(
      "Container not initialized. Call initializeContainer() first or inject container explicitly."
    );
  }
  return globalContainer;
}

/**
 * Clear the global container (for testing)
 */
export function clearContainer(): void {
  globalContainer = null;
}
