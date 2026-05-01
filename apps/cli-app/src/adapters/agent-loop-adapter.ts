/**
 * Agent Loop Adapter
 * Encapsulates SDK API calls related to Agent Loop
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  AgentLoopFactory,
  AgentLoopCoordinator,
  AgentLoopRegistry,
  AgentLoopExecutor,
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopEntityOptions,
  type AgentLoopCheckpointDependencies,
  type AgentLoopEntity,
} from "@wf-agent/sdk/agent";
import type { AgentLoopConfig, AgentLoopResult, ID } from "@wf-agent/types";
import { LLMExecutor, LLMWrapper, ToolRegistry, EventRegistry } from "@wf-agent/sdk/core";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Agent Loop Adapter
 */
export class AgentLoopAdapter extends BaseAdapter {
  private coordinator: AgentLoopCoordinator;
  private registry: AgentLoopRegistry;
  private eventRegistry: EventRegistry;

  constructor() {
    super();
    // Initialize registry, event registry and coordinator
    this.registry = new AgentLoopRegistry();
    this.eventRegistry = new EventRegistry();

    const llmWrapper = new LLMWrapper(this.eventRegistry);
    const llmExecutor = new LLMExecutor(llmWrapper);
    const toolRegistry = new ToolRegistry();
    const executor = new AgentLoopExecutor(llmExecutor, toolRegistry, this.eventRegistry);
    this.coordinator = new AgentLoopCoordinator(this.registry, executor);
  }

  /**
   * Create Agent Loop instance
   * @param config Loop configuration
   * @param options Creation options
   */
  async createAgentLoop(
    config: AgentLoopConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const entity = await AgentLoopFactory.create(config, options);
      this.registry.register(entity);

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
    config: AgentLoopConfig,
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
    config: AgentLoopConfig,
    options: AgentLoopEntityOptions = {},
    onEvent?: (event: any) => void,
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
        // AgentStreamEvent has enum values like 'agent_end', 'agent_error'
        // MessageStreamEvent has type values like 'error', 'end'
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
  async startAgentLoop(config: AgentLoopConfig, options: AgentLoopEntityOptions = {}): Promise<ID> {
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
  getAgentLoop(id: ID): any | undefined {
    const entity = this.coordinator.get(id);
    if (!entity) {
      return undefined;
    }

    return {
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      variables: entity.getAllVariables(),
      messageCount: entity.getMessages().length,
    };
  }

  /**
   * Get Agent Loop entity (internal use)
   * @param id Instance ID
   */
  getAgentLoopEntity(id: ID): any | undefined {
    return this.coordinator.get(id);
  }

  /**
   * Get Agent Loop status
   * @param id Instance ID
   */
  getAgentLoopStatus(id: ID): string | undefined {
    return this.coordinator.getStatus(id);
  }

  /**
   * List all Agent Loop instances
   */
  listAgentLoops(): any[] {
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
  listRunningAgentLoops(): any[] {
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
  listPausedAgentLoops(): any[] {
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
    metadata?: any,
  ): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const entity = this.registry.get(id);
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
   * @param dependencies Checkpoint dependencies
   */
  async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: AgentLoopCheckpointDependencies,
  ): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const entity = await AgentLoopFactory.fromCheckpoint(checkpointId, dependencies);
      this.registry.register(entity);

      this.output.infoLog(`Agent Loop restored from checkpoint: ${entity.id}`);
      return { id: entity.id };
    }, "Restore from a checkpoint");
  }

  /**
   * Clone Agent Loop
   * @param id Instance ID
   */
  cloneAgentLoop(id: ID): Promise<{ id: ID }> {
    return this.executeWithErrorHandling(async () => {
      const entity = this.registry.get(id);
      if (!entity) {
        throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
      }

      const cloned = cloneAgentLoop(entity);
      this.registry.register(cloned);

      this.output.infoLog(`Agent Loop cloned: ${cloned.id}`);
      return { id: cloned.id };
    }, "Clone Agent Loop");
  }

  /**
   * Cleanup Agent Loop resources
   * @param id Instance ID
   */
  cleanupAgentLoop(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const entity = this.registry.get(id);
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
  getAgentLoopMessages(id: ID): any[] {
    const entity = this.registry.get(id);
    if (!entity) {
      throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
    }

    return entity.getMessages();
  }

  /**
   * Get Agent Loop variables
   * @param id Instance ID
   */
  getAgentLoopVariables(id: ID): Record<string, any> {
    const entity = this.registry.get(id);
    if (!entity) {
      throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
    }

    return entity.getAllVariables();
  }

  /**
   * Set Agent Loop variable
   * @param id Instance ID
   * @param name Variable name
   * @param value Variable value
   */
  setAgentLoopVariable(id: ID, name: string, value: any): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const entity = this.registry.get(id);
      if (!entity) {
        throw new CLINotFoundError(`Agent Loop not found: ${id}`, "AgentLoop", id);
      }

      entity.setVariable(name, value);
      this.output.infoLog(`Variable set: ${name}`);
    }, "Set variables");
  }
}
