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
  AgentStateCoordinator,
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopEntityOptions,
  type AgentLoopCheckpointDependencies,
  type AgentLoopEntity,
} from "@wf-agent/sdk/agent";
import {
  EventRegistry,
  LLMWrapper,
  LLMExecutor,
  ToolRegistry,
  SkillRegistry,
  AgentProfileRegistry,
  ServiceIdentifiers,
  injectSkillMetadata,
  isToolAvailable,
  METADATA_TOOL_NAMES,
  ConversationSession,
} from "@wf-agent/sdk/core";
import type {
  AgentLoopRuntimeConfig,
  AgentLoopResult,
  ID,
  Message,
  AgentStreamEvent,
  MessageStreamEvent,
  AgentToolConfig,
} from "@wf-agent/types";
import type {
  SkillHandlerConfig,
  PredefinedToolsOptions,
  WorkflowHandlerConfig,
  AgentHandlerConfig,
} from "@wf-agent/sdk/resources";
import { createPredefinedTools, createBuiltinTools } from "@wf-agent/sdk/resources";
import { toSdkTool } from "@wf-agent/sdk/services";
import { loadAgentLoopConfig } from "@wf-agent/config-processor";
import { CLINotFoundError } from "../types/cli-types.js";
import { CLIToolApprovalHandler } from "../handlers/user-interaction/tool-approval.js";

/** Local SkillInfo type matching the one in skill handler config. */
interface SkillInfo {
  name: string;
  description: string;
}

/**
 * Agent Loop Adapter
 */
export class AgentLoopAdapter extends BaseAdapter {
  private coordinator: AgentLoopCoordinator;
  private registry: AgentLoopRegistry;
  private eventRegistry: EventRegistry;
  private toolRegistry: ToolRegistry;

  constructor() {
    super();
    // Initialize registry, event registry and coordinator
    this.registry = new AgentLoopRegistry();
    this.eventRegistry = new EventRegistry();

    const llmWrapper = new LLMWrapper(this.eventRegistry);
    const llmExecutor = new LLMExecutor(llmWrapper);
    this.toolRegistry = new ToolRegistry();
    const toolApprovalHandler = new CLIToolApprovalHandler();

    const executor = new AgentLoopExecutor({
      llmExecutor,
      toolService: this.toolRegistry,
      eventManager: this.eventRegistry,
      toolApprovalHandler,
    });

    // Get globalContext from SDK instance
    const globalContext = this.sdk.getGlobalContext();
    this.coordinator = new AgentLoopCoordinator(this.registry, executor, globalContext);
  }

  /**
   * Apply skills integration to the agent loop runtime config.
   * Resolves SkillRegistry from the SDK's DI container,
   * injects skill metadata into the system prompt, and registers
   * the skill tool with proper handler on the internal ToolRegistry.
   *
   * Only injects skill metadata if:
   * 1. 'skill' tool is in availableTools, OR
   * 2. Skills exist and autoAddTool is enabled (default behavior)
   *
   * Safe to call even if skills are not configured — a no-op in that case.
   *
   * @param config The runtime config to apply skills integration to
   */
  applySkillsToConfig(config: AgentLoopRuntimeConfig): void {
    try {
      const globalContext = this.sdk.getGlobalContext();
      const container = globalContext.container;
      const skillRegistry = container.get(ServiceIdentifiers.SkillRegistry) as SkillRegistry | undefined;

      // Use unified metadata injection utility
      const skillResult = injectSkillMetadata(skillRegistry, {
        systemPrompt: config.systemPrompt || "",
        availableTools: config.availableTools,
        autoAddTool: true, // Auto-add skill tool if skills exist
      });

      // Update config with results
      config.systemPrompt = skillResult.systemPrompt;
      // Type assertion: injectSkillMetadata preserves AgentToolConfig type when input is AgentToolConfig
      config.availableTools = skillResult.availableTools as AgentToolConfig | undefined;

      // If skill metadata was injected, register the skill tool handler
      if (skillResult.injected) {
        // Build a SkillHandlerConfig so the skill tool handler can resolve skills
        const skillHandlerConfig = this.buildSkillHandlerConfig(skillRegistry!);
        if (skillHandlerConfig) {
          // Create predefined tools with the skill config and register on the internal ToolRegistry
          const options: PredefinedToolsOptions = {
            config: { skill: skillHandlerConfig },
          };
          const tools = createPredefinedTools(options);
          for (const tool of tools) {
            this.toolRegistry.registerTool(toSdkTool(tool), { skipIfExists: true });
          }
          this.output.infoLog(
            `Registered ${tools.length} predefined tools with skill support on agent loop ToolRegistry`,
          );
        }

        this.output.infoLog(`Skill metadata injected into system prompt (${skillResult.skillCount} skills)`);
      } else if (skillResult.skillCount > 0) {
        // Skills exist but not injected (tool not in availableTools and autoAddTool was false)
        this.output.infoLog(`Skills available (${skillResult.skillCount}) but 'skill' tool not in availableTools`);
      }
    } catch {
      this.output.infoLog("Skills not configured, running without skill support");
    }
  }

  /**
   * Build a SkillHandlerConfig from a SkillRegistry instance.
   */
  private buildSkillHandlerConfig(skillRegistry: SkillRegistry): SkillHandlerConfig | null {
    try {
      const allSkills = skillRegistry.getAllSkills();
      const skillMap = new Map(allSkills.map(s => [s.name, s]));

      return {
        loader: {
          getAvailableSkills: (): SkillInfo[] => {
            return allSkills.map(s => ({ name: s.name, description: s.description }));
          },
          hasSkill: (name: string): boolean => {
            return skillMap.has(name);
          },
          loadContent: async (
            name: string,
            variables?: Record<string, unknown>,
          ): Promise<string> => {
            const result = await skillRegistry.loadContent(name, {
              variables,
            });
            if (!result.success || !result.content) {
              throw new Error(result.error?.message || `Failed to load skill: ${name}`);
            }
            return result.content;
          },
        },
      };
    } catch {
      return null;
    }
  }

  /**
   * Apply workflows integration to the agent loop runtime config.
   *
   * Resolves the WorkflowRegistry from the SDK's DI container, filters workflows
   * based on the agent config's allowedWorkflows list, injects workflow metadata
   * into the system prompt (Level 1: discovery), and registers the builtin workflow
   * tools with a WorkflowHandlerConfig on the internal ToolRegistry.
   *
   * Workflow visibility is controlled by the static agent definition (allowedWorkflows).
   * Only workflows explicitly allowed by the agent config will be visible to the LLM.
   *
   * Only injects workflow metadata if:
   * 1. 'execute_workflow' tool is in availableTools, OR
   * 2. allowedWorkflows are configured (auto-adds execute_workflow tool)
   *
   * Safe to call even if no workflows are configured — a no-op in that case.
   *
   * @param config The runtime config to apply workflows integration to
   */
  async applyWorkflowsToConfig(config: AgentLoopRuntimeConfig): Promise<void> {
    try {
      // Check if execute_workflow tool is available or should be added
      const hasExecuteWorkflowTool = isToolAvailable(
        config.availableTools,
        METADATA_TOOL_NAMES.EXECUTE_WORKFLOW,
      );

      // Determine allowed workflow IDs from config
      const allowedIds = config.availableTools?.allowedWorkflows;
      if (!allowedIds || allowedIds.length === 0) {
        // No allowedWorkflows configured
        if (hasExecuteWorkflowTool) {
          this.output.infoLog(
            "'execute_workflow' tool is available but no allowedWorkflows configured",
          );
        }
        return;
      }

      const globalContext = this.sdk.getGlobalContext();
      const container = globalContext.container;
      const workflowRegistry = container.get(ServiceIdentifiers.WorkflowRegistry) as {
        get: (id: string) =>
          | {
              id: string;
              name: string;
              description?: string;
              type: string;
              variables?: Array<{ name: string }>;
              metadata?: { tags?: string[]; category?: string };
            }
          | undefined;
        getAll?: () => Array<{
          id: string;
          name: string;
          description?: string;
          type: string;
          variables?: Array<{ name: string }>;
          metadata?: { tags?: string[]; category?: string };
        }>;
        list: () => Promise<Array<{ id: string; name: string }>>;
      } | null;

      if (!workflowRegistry) {
        this.output.infoLog("Workflow registry not available, running without workflow support");
        return;
      }

      // Resolve workflows based on allowedIds
      const workflows: Array<{
        id: string;
        name: string;
        description?: string;
        type: string;
        variables?: Array<{ name: string }>;
        metadata?: { tags?: string[]; category?: string };
      }> = [];

      const isWildcard = allowedIds.length === 1 && allowedIds[0] === "*";
      if (isWildcard) {
        // Use getAll from memory cache (if available) or list from storage
        try {
          const summaryList = await workflowRegistry.list();
          for (const summary of summaryList) {
            const wf = workflowRegistry.get(summary.id);
            if (wf) {
              workflows.push(wf);
            }
          }
        } catch {
          this.output.infoLog("Failed to list workflows from registry");
          return;
        }
      } else {
        for (const id of allowedIds) {
          const wf = workflowRegistry.get(id);
          if (wf) {
            workflows.push(wf);
          }
        }
      }

      if (workflows.length === 0) {
        this.output.infoLog("No workflows found for allowedWorkflows configuration");
        return;
      }

      // Inject workflow metadata into system prompt (Level 1: discovery)
      const workflowPrompt = this.buildWorkflowSystemPrompt(workflows);
      config.systemPrompt = config.systemPrompt
        ? `${config.systemPrompt}\n\n${workflowPrompt}`
        : workflowPrompt;

      // Ensure "execute_workflow" tool is in availableTools
      if (!config.availableTools) {
        config.availableTools = { tools: ["execute_workflow"] };
      } else if (
        config.availableTools.tools &&
        !config.availableTools.tools.includes("execute_workflow")
      ) {
        config.availableTools.tools = [...config.availableTools.tools, "execute_workflow"];
      }

      // Build a WorkflowHandlerConfig so the execute_workflow tool handler can resolve workflows
      const workflowHandlerConfig = this.buildWorkflowHandlerConfig(workflows);
      if (workflowHandlerConfig) {
        // Create builtin tools with the workflow config and register on the internal ToolRegistry
        const builtinOptions = { workflow: workflowHandlerConfig };
        const builtinTools = createBuiltinTools(builtinOptions);
        for (const tool of builtinTools) {
          this.toolRegistry.registerTool(tool, { skipIfExists: true });
        }
        this.output.infoLog(
          `Registered ${builtinTools.length} builtin tools with workflow support on agent loop ToolRegistry`,
        );
      }

      this.output.infoLog("Workflow metadata injected into system prompt");
    } catch {
      this.output.infoLog("Workflows not configured, running without workflow support");
    }
  }

  /**
   * Build a formatted system prompt section for available workflows.
   */
  private buildWorkflowSystemPrompt(
    workflows: Array<{
      id: string;
      name: string;
      description?: string;
      type: string;
      variables?: Array<{ name: string }>;
      metadata?: { tags?: string[]; category?: string };
    }>,
  ): string {
    const lines: string[] = [
      "## Available Workflows",
      "",
      "You can execute the following workflows using the execute_workflow tool:",
      "",
    ];

    for (const wf of workflows) {
      lines.push(`- **${wf.id}**: ${wf.description || wf.name}`);
      if (wf.variables && wf.variables.length > 0) {
        const inputs = wf.variables.map(v => `\`${v.name}\``).join(", ");
        lines.push(`  - Inputs: ${inputs}`);
      }
    }

    lines.push("");
    lines.push(
      "Use `execute_workflow` with the workflow ID and required input parameters to run a workflow.",
    );

    return lines.join("\n");
  }

  /**
   * Build a WorkflowHandlerConfig from a list of allowed workflows.
   */
  private buildWorkflowHandlerConfig(
    workflows: Array<{
      id: string;
      name: string;
      description?: string;
      type: string;
      variables?: Array<{ name: string }>;
      metadata?: { tags?: string[]; category?: string };
    }>,
  ): WorkflowHandlerConfig | null {
    try {
      const workflowMap = new Map(workflows.map(w => [w.id, w]));

      return {
        loader: {
          getAvailableWorkflows: () => {
            return workflows.map(w => ({
              id: w.id,
              name: w.name,
              description: w.description || "",
              type: w.type as import("@wf-agent/types").WorkflowTemplateType,
              variables: w.variables,
              tags: w.metadata?.tags,
              category: w.metadata?.category,
            }));
          },
          hasWorkflow: (id: string): boolean => {
            return workflowMap.has(id);
          },
          loadDefinition: async (id: string) => {
            const wf = workflowMap.get(id);
            if (!wf) {
              throw new Error(`Workflow '${id}' not found in allowed workflows`);
            }
            return { id: wf.id, name: wf.name, description: wf.description };
          },
        },
      };
    } catch {
      return null;
    }
  }

  /**
   * Apply agent integration to the agent loop runtime config.
   *
   * Registers the call_agent builtin tool with an AgentHandlerConfig,
   * pulling agent profile metadata from the AgentProfileRegistry in the
   * DI container. This enables description injection so the LLM can
   * discover available agent profiles.
   *
   * Only applies agent integration if 'call_agent' tool is in availableTools.
   *
   * Safe to call even if no agent profiles are registered — the tool
   * remains available with a default description.
   *
   * @param config The runtime config to apply agent integration to
   */
  applyAgentsToConfig(config: AgentLoopRuntimeConfig): void {
    try {
      // Check if call_agent tool is available using unified utility
      const hasCallAgent = isToolAvailable(
        config.availableTools,
        METADATA_TOOL_NAMES.CALL_AGENT,
      );
      if (!hasCallAgent) {
        this.output.infoLog("'call_agent' tool not in availableTools, skipping agent integration");
        return;
      }

      // Build an AgentHandlerConfig (agent profiles can be added later via registry)
      const agentHandlerConfig = this.buildAgentHandlerConfig();

      if (agentHandlerConfig) {
        // Create builtin tools with the agent config and register on the internal ToolRegistry
        const builtinOptions = { agent: agentHandlerConfig };
        const builtinTools = createBuiltinTools(builtinOptions);
        for (const tool of builtinTools) {
          this.toolRegistry.registerTool(tool, { skipIfExists: true });
        }
        this.output.infoLog(
          `Registered ${builtinTools.length} builtin tools with agent support on agent loop ToolRegistry`,
        );
      }
    } catch {
      this.output.infoLog(
        "Agent integration not configured, running without agent profile injection",
      );
    }
  }

  /**
   * Build an AgentHandlerConfig from the AgentProfileRegistry in the DI container.
   *
   * The returned config provides the call_agent tool handler with access to:
   * - getAvailableAgentProfiles() - list all profiles for description injection and validation
   * - hasAgentProfile(id) - validate profile existence before execution
   *
   * @returns AgentHandlerConfig if AgentProfileRegistry is available, null otherwise
   */
  private buildAgentHandlerConfig(): AgentHandlerConfig | null {
    try {
      const globalContext = this.sdk.getGlobalContext();
      const container = globalContext.container;
      const registry = container.get<AgentProfileRegistry>(ServiceIdentifiers.AgentProfileRegistry);
      if (!registry) {
        this.output.infoLog(
          "AgentProfileRegistry not found in DI container, agent profiles unavailable",
        );
        return null;
      }
      return {
        loader: {
          getAvailableAgentProfiles: () => {
            return registry.list().map(p => ({
              id: p.id,
              name: p.name,
              description: p.description,
            }));
          },
          hasAgentProfile: (id: string): boolean => {
            return registry.has(id);
          },
          // Inject loadAgentLoopConfig from application layer
          loadAgentLoopConfig,
        },
      };
    } catch (error) {
      this.output.infoLog(
        "AgentProfileRegistry not available: " +
          (error instanceof Error ? error.message : String(error)),
      );
      return null;
    }
  }

  /**
   * Create a SkillHandlerConfig from the SkillRegistry in the DI container.
   *
   * The returned config provides the `skill` tool handler with access to:
   * - getAvailableSkills() - list all skills for error messages
   * - hasSkill(name) - validate skill existence before loading
   * - loadContent(name, variables) - load full skill content on demand
   *
   * This is the public interface for external callers (e.g., CLI ToolAdapter).
   *
   * @returns SkillHandlerConfig if SkillRegistry is available, null otherwise
   */
  createSkillHandlerConfig(): SkillHandlerConfig | null {
    try {
      const globalContext = this.sdk.getGlobalContext();
      const container = globalContext.container;
      const skillRegistry = container.get<SkillRegistry>(ServiceIdentifiers.SkillRegistry);
      if (!skillRegistry) {
        return null;
      }
      return this.buildSkillHandlerConfig(skillRegistry);
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
      const conversationSession = new ConversationSession({ executionId: entity.id });
      const stateCoordinator = new AgentStateCoordinator({ conversationManager: conversationSession });
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
