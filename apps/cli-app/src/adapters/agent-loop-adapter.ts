/**
 * Agent Loop Adapter
 * Encapsulates SDK API calls related to Agent Loop
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  AgentLoopFactory,
  AgentLoopCoordinator,
  AgentLoopRegistry,
  AgentStateCoordinator,
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopEntityOptions,
  type AgentLoopCheckpointDependencies,
  type AgentLoopEntity,
} from "@wf-agent/sdk/agent";
import { CLINotFoundError } from "../types/cli-types.js";
import type {
  AgentLoopRuntimeConfig,
  AgentLoopResult,
  ID,
  Message,
  AgentStreamEvent,
  MessageStreamEvent,
  DynamicContextConfig,
} from "@wf-agent/types";

/**
 * Agent Loop Adapter
 */
export class AgentLoopAdapter extends BaseAdapter {
  private coordinator: AgentLoopCoordinator;
  private registry: AgentLoopRegistry;

  constructor() {
    super();
    // Initialize registry and coordinator
    this.registry = new AgentLoopRegistry();

    // Get globalContext from SDK instance
    const globalContext = this.sdk.getGlobalContext();

    // Create a minimal executor configuration for the coordinator
    // The actual tool execution and LLM integration will be handled by the SDK
    this.coordinator = new AgentLoopCoordinator(this.registry, undefined as any, globalContext);
  }

  /**
   * Apply dynamic context injection to agent loop runtime config.
   *
   * @param config The runtime config to apply dynamic context to
   * @param options CLI/runtime override options
   */
  async applyDynamicContextToConfig(
    config: AgentLoopRuntimeConfig,
    options?: Partial<DynamicContextConfig>,
  ): Promise<void> {
    try {
      const { buildDynamicPromptInjection } = await import("@wf-agent/sdk/resources");

      // Merge configuration: agent config + CLI overrides
      const mergedConfig: DynamicContextConfig = {
        // Start with agent's static configuration
        ...config.dynamicContextConfig,
        // Override with CLI options
        ...options,
      };

      // Create the transformContext function
      config.transformContext = async (context) => {
        return buildDynamicPromptInjection(context, mergedConfig);
      };

      this.output.infoLog("Dynamic context injection enabled");
    } catch (error) {
      this.output.infoLog(
        "Dynamic context not configured: " +
          (error instanceof Error ? error.message : String(error)),
      );
    }
  }

  /**
   * Apply skills integration to the agent loop runtime config.
   */
  applySkillsToConfig(): void {
    try {
      this.output.infoLog("Skills integration not configured in CLI adapter");
    } catch {
      this.output.infoLog("Skills not configured, running without skill support");
    }
  }

  /**
   * Apply workflows integration to the agent loop runtime config.
   */
  async applyWorkflowsToConfig(): Promise<void> {
    try {
      this.output.infoLog("Workflow integration not configured in CLI adapter");
    } catch {
      this.output.infoLog("Workflows not configured, running without workflow support");
    }
  }

  /**
   * Apply agent integration to the agent loop runtime config.
   */
  applyAgentsToConfig(): void {
    try {
      this.output.infoLog("Agent integration not configured in CLI adapter");
    } catch {
      this.output.infoLog("Agent integration not configured, running without agent profile injection");
    }
  }

  /**
   * Create a SkillHandlerConfig from the SkillRegistry in the DI container.
   */
  createSkillHandlerConfig(): unknown {
    try {
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Create Agent Loop instance
   * @param config Loop configuration
   * @param options Creation options
   */
  async createAgentLoop(
    config: AgentLoopRuntimeConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const globalContext = this.sdk.getGlobalContext();
      const { entity, stateCoordinator } = await AgentLoopFactory.create(globalContext, config, options);
      this.registry.register(entity);
      this.registry.registerStateCoordinator(entity.id, stateCoordinator);

      this.output.infoLog(`Agent Loop created successfully: ${entity.id}`);
      return { id: entity.id };
    }, "Create an Agent Loop");
  }

  /**
   * Execute Agent Loop (synchronous)
   * @param config Loop configuration
   * @param options Execution options
   */
  async executeAgentLoop(
    config: AgentLoopRuntimeConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<AgentLoopResult> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.coordinator.execute(config, options);

      if (result.success) {
        this.output.infoLog(`Agent Loop execution completed`);
      } else {
        this.output.errorLog(`Agent Loop execution failed: ${result.error}`);
      }

      return result;
    }, "Execute the Agent Loop");
  }

  /**
   * Stream execution of Agent Loop
   * @param config Loop configuration
   * @param options Execution options
   * @param onEvent Event callback
   */
  async executeAgentLoopStream(
    config: AgentLoopRuntimeConfig,
    options: AgentLoopEntityOptions = {},
    onEvent?: (event: AgentStreamEvent | MessageStreamEvent) => void,
  ): Promise<AgentLoopResult> {
    return this.executeWithErrorHandling(async () => {
      let lastResult: AgentLoopResult = {
        success: false,
        iterations: 0,
        toolCallCount: 0,
      };

      for await (const event of this.coordinator.executeStream(config, options)) {
        if (onEvent) {
          onEvent(event);
        }

        // Track the last complete/error event for result
        if (event.type === "agent_end") {
          const endEvent = event as {
            type: "agent_end";
            success: boolean;
            iterations: number;
            toolCallCount: number;
            error?: unknown;
          };
          lastResult = {
            success: endEvent.success,
            iterations: endEvent.iterations,
            toolCallCount: endEvent.toolCallCount,
            error: endEvent.error,
          };
        } else if (event.type === "agent_error") {
          const errorEvent = event as { type: "agent_error"; error: unknown; iteration: number };
          lastResult = {
            success: false,
            iterations: errorEvent.iteration,
            toolCallCount: 0,
            error: errorEvent.error,
          };
        }
      }

      return lastResult;
    }, "Stream execution of the Agent Loop");
  }

  /**
   * Asynchronously start Agent Loop
   * @param config Loop configuration
   * @param options Execution options
   */
  async startAgentLoop(
    config: AgentLoopRuntimeConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<ID> {
    return this.executeWithErrorHandling(async () => {
      const id = await this.coordinator.start(config, options);
      this.output.infoLog(`Agent Loop started: ${id}`);
      return id;
    }, "Start the Agent Loop");
  }

  /**
   * Pause Agent Loop
   * @param id Instance ID
   */
  async pauseAgentLoop(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.coordinator.pause(id);
      this.output.infoLog(`Agent Loop paused: ${id}`);
    }, "Pause Agent Loop");
  }

  /**
   * Resume Agent Loop
   * @param id Instance ID
   */
  async resumeAgentLoop(id: ID): Promise<AgentLoopResult> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.coordinator.resume(id);
      this.output.infoLog(`Agent Loop resumed: ${id}`);
      return result;
    }, "Resume Agent Loop");
  }

  /**
   * Stop Agent Loop
   * @param id Instance ID
   */
  async stopAgentLoop(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.coordinator.stop(id);
      this.output.infoLog(`Agent Loop stopped: ${id}`);
    }, "Stop Agent Loop");
  }

  /**
   * Get Agent Loop instance
   * @param id Instance ID
   */
  async getAgentLoop(id: ID): Promise<
    | {
        id: ID;
        status: string;
        currentIteration: number;
        toolCallCount: number;
        messageCount: number;
      }
    | undefined
  > {
    const entity = await this.coordinator.get(id);
    if (!entity) {
      return undefined;
    }

    const stateCoordinator = this.registry.getStateCoordinator(entity.id);
    const messages = stateCoordinator?.getMessages() ?? [];

    return {
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      messageCount: messages.length,
    };
  }

  /**
   * Get Agent Loop entity (internal use)
   * @param id Instance ID
   */
  async getAgentLoopEntity(id: ID): Promise<AgentLoopEntity | undefined> {
    return await this.coordinator.get(id);
  }

  /**
   * Get Agent Loop status
   * @param id Instance ID
   */
  async getAgentLoopStatus(id: ID): Promise<string | undefined> {
    const status = await this.coordinator.getStatus(id);
    return status;
  }

  /**
   * List all Agent Loop instances
   */
  listAgentLoops(): Array<{
    id: ID;
    status: string;
    currentIteration: number;
    toolCallCount: number;
  }> {
    const entities = this.registry.getAll();
    return entities.map((entity: AgentLoopEntity) => ({
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
    }));
  }

  /**
   * List running Agent Loops
   */
  listRunningAgentLoops(): Array<{
    id: ID;
    status: string;
    currentIteration: number;
    toolCallCount: number;
  }> {
    const entities = this.coordinator.getRunning();
    return entities.map((entity: AgentLoopEntity) => ({
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
    }));
  }

  /**
   * List paused Agent Loops
   */
  listPausedAgentLoops(): Array<{
    id: ID;
    status: string;
    currentIteration: number;
    toolCallCount: number;
  }> {
    const entities = this.coordinator.getPaused();
    return entities.map((entity: AgentLoopEntity) => ({
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
    }));
  }

  /**
   * Cleanup completed Agent Loops
   */
  cleanupAgentLoops(): number {
    const count = this.coordinator.cleanup();
    this.output.infoLog(`Cleaned up ${count} completed Agent Loop(s)`);
    return count;
  }

  /**
   * Create Agent Loop checkpoint
   * @param id Instance ID
   * @param dependencies Checkpoint dependencies
   * @param metadata Metadata
   */
  async createCheckpoint(
    id: ID,
    dependencies: AgentLoopCheckpointDependencies,
    metadata?: Record<string, unknown>,
  ): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const entity = await this.registry.get(id);
      if (!entity) {
        throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
      }

      const checkpointId = await createAgentLoopCheckpoint(entity, dependencies, { metadata });
      this.output.infoLog(`Checkpoint created: ${checkpointId}`);
      return checkpointId;
    }, "Create a checkpoint");
  }

  /**
   * Restore Agent Loop from checkpoint
   * @param checkpointId Checkpoint ID
   * @param config Agent loop config
   * @param dependencies Checkpoint dependencies
   */
  async restoreFromCheckpoint(
    checkpointId: string,
    config: AgentLoopRuntimeConfig,
    dependencies: AgentLoopCheckpointDependencies,
  ): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const entity = await AgentLoopFactory.fromCheckpoint(checkpointId, config, dependencies);
      this.registry.register(entity);

      // Create AgentStateCoordinator for the restored entity
      const stateCoordinator = new AgentStateCoordinator({ conversationManager: undefined as any });
      this.registry.registerStateCoordinator(entity.id, stateCoordinator);

      this.output.infoLog(`Agent Loop restored from checkpoint: ${entity.id}`);
      return { id: entity.id };
    }, "Restore from a checkpoint");
  }

  /**
   * Clone Agent Loop
   * @param id Instance ID
   */
  async cloneAgentLoop(id: ID): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const entity = await this.registry.get(id);
      if (!entity) {
        throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
      }

      const stateCoordinator = this.registry.getStateCoordinator(entity.id);
      if (!stateCoordinator) {
        throw new CLINotFoundError(`Agent State Coordinator not found for: ${id}`, "AgentLoop", id);
      }

      const cloned = cloneAgentLoop(entity, stateCoordinator);
      this.registry.register(cloned.entity);
      this.registry.registerStateCoordinator(cloned.entity.id, cloned.stateCoordinator);

      this.output.infoLog(`Agent Loop cloned: ${cloned.entity.id}`);
      return { id: cloned.entity.id };
    }, "Clone Agent Loop");
  }

  /**
   * Cleanup Agent Loop resources
   * @param id Instance ID
   */
  async cleanupAgentLoop(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const entity = await this.registry.get(id);
      if (!entity) {
        throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
      }

      cleanupAgentLoop(entity);
      this.registry.unregister(id);

      this.output.infoLog(`Agent Loop cleaned up: ${id}`);
    }, "Clean up the Agent Loop");
  }

  /**
   * Get Agent Loop message history
   * @param id Instance ID
   */
  async getAgentLoopMessages(id: ID): Promise<Message[]> {
    const entity = await this.registry.get(id);
    if (!entity) {
      throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
    }

    const stateCoordinator = this.registry.getStateCoordinator(entity.id);
    return stateCoordinator?.getMessages() ?? [];
  }
}
