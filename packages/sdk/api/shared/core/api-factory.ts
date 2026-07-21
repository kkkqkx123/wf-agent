/**
 * APIFactory - An API factory class that manages the creation of all resource API instances in a centralized manner.
 *
 * Design patterns used:
 * - Factory pattern: Responsible for creating API instances in a unified manner.
 * - Instance-level caching: Ensures that each API type has only one instance per APIFactory.
 * - Consistent Cache Management: All APIs use the same caching strategy
 * - Registry pattern: API constructors are registered in a central registry, reducing boilerplate
 *   when adding new APIs (no need to add a new createXxx method for each new API)
 *
 * Architecture:
 * - Each SDKInstance creates its own APIFactory
 * - APIFactory caches API instances internally (instance-level singleton)
 * - Multiple SDKInstances can coexist with fully isolated API factories
 *
 * Cache Strategy:
 * - All APIs use the same createAPI() method for consistency
 * - Some APIs don't require dependencies; they use a wrapper approach
 * - No hand-written cache logic - all delegated to createAPI()
 *
 * Usage:
 * ```typescript
 * // Create via specific method (traditional)
 * const workflowAPI = factory.createWorkflowAPI();
 *
 * // Create via generic getAPI method (recommended for new APIs)
 * const workflowAPI = factory.getAPI("workflows", WorkflowRegistryAPI);
  * ```
  */

  import { WorkflowRegistryAPI } from "../../workflow/resources/workflow-registry-api.js";
import { ToolRegistryAPI } from "../resources/tools/tool-registry-api.js";
import { WorkflowExecutionRegistryAPI } from "../../workflow/resources/workflow-execution-registry-api.js";
import { ScriptRegistryAPI } from "../resources/scripts/script-registry-api.js";
import { LLMProfileRegistryAPI } from "../resources/llm/llm-profile-registry-api.js";
import { NodeRegistryAPI } from "../../workflow/resources/node-template-registry-api.js";
import { TriggerTemplateRegistryAPI } from "../../workflow/resources/trigger-template-registry-api.js";
import { UserInteractionResourceAPI } from "../../workflow/resources/user-interaction-resource-api.js";
import { EventResourceAPI } from "../resources/events/event-resource-api.js";
import { TriggerResourceAPI } from "../../workflow/resources/trigger-resource-api.js";
import { VariableResourceAPI } from "../../workflow/resources/variable-resource-api.js";
import { MessageResourceAPI } from "../../workflow/resources/message-resource-api.js";
import { SkillRegistryAPI } from "../resources/skills/skill-registry-api.js";
import { MetricsResourceAPI } from "../resources/metrics/metrics-resource-api.js";
import { TaskResourceAPI } from "../resources/tasks/task-resource-api.js";
import { WorkflowGraphQueryAPI } from "../resources/graphs/workflow-graph-query-api.js";
import { StorageDiagnosticsAPI } from "../resources/diagnostics/storage-diagnostics-api.js";
import { SearchAPI } from "../resources/search/search-api.js";
import { FileCheckpointResourceAPI } from "../../workflow/resources/file-checkpoint-resource-api.js";
import { AgentLoopRegistryAPI } from "../../agent/resources/agent-loop-registry-api.js";
import { AgentLoopResourceAPI } from "../../agent/resources/agent-loop-resource-api.js";
import { AgentLoopCheckpointResourceAPI } from "../../agent/resources/checkpoint-resource-api.js";
import { AgentLoopMessageResourceAPI } from "../../agent/resources/message-resource-api.js";
import { AgentLoopIterationAPI } from "../../agent/resources/agent-loop-iteration-api.js";
import { AgentVariableResourceAPI } from "../../agent/resources/agent-variable-resource-api.js";
import { AgentUserInteractionResourceAPI } from "../../agent/resources/agent-user-interaction-resource-api.js";
import { AgentErrorAnalysisAPI } from "../../agent/resources/errors/agent-error-analysis-api.js";
import { AgentPerformanceAnalysisAPI } from "../../agent/resources/agent-performance-analysis-api.js";
import { AgentExecutionRegistryAPI } from "../../agent/resources/agent-execution-registry-api.js";
import { AgentExecutionStateAPI } from "../../agent/resources/agent-execution-state-api.js";
import { AgentExecutionGraphQueryAPI } from "../../agent/resources/agent-execution-graph-query-api.js";
import { AgentTriggerResourceAPI } from "../../agent/resources/agent-trigger-resource-api.js";
import { AgentTriggerTemplateRegistryAPI } from "../../agent/resources/agent-trigger-template-registry-api.js";
import { AgentHookTemplateRegistryAPI } from "../../agent/resources/agent-hook-template-registry-api.js";
import { AgentTemplateRegistryAPI } from "../../agent/resources/agent-template-registry-api.js";
import { APIDependencyManager } from "./sdk-dependencies.js";
import { ExecutionEventLogger } from "../../../shared/events/execution-event-logger.js";

/**
 * Collection of all API instances
 */
export interface AllAPIs {
  /** Workflow API */
  workflows: WorkflowRegistryAPI;
  /** Tool API */
  tools: ToolRegistryAPI;
  /** Workflow Execution API */
  executions: WorkflowExecutionRegistryAPI;
  /** Script API */
  scripts: ScriptRegistryAPI;
  /** Profile API */
  profiles: LLMProfileRegistryAPI;
  /** Node Template API */
  nodeTemplates: NodeRegistryAPI;
  /** Trigger Template API */
  triggerTemplates: TriggerTemplateRegistryAPI;
  /** User Interaction API */
  userInteractions: UserInteractionResourceAPI;
  /** Event API */
  events: EventResourceAPI;
  /** Trigger API */
  triggers: TriggerResourceAPI;
  /** Variable API */
  variables: VariableResourceAPI;
  /** Message API */
  messages: MessageResourceAPI;
  /** Skill API */
  skills: SkillRegistryAPI;
  /** Metrics API */
  metrics: MetricsResourceAPI;
  /** Task API */
  tasks: TaskResourceAPI;
  /** Workflow Graph Query API */
  graphs: WorkflowGraphQueryAPI;
  /** Storage Diagnostics API */
  diagnostics: StorageDiagnosticsAPI;
  /** Search API */
  search: SearchAPI;
  /** File Checkpoint API */
  fileCheckpoints: FileCheckpointResourceAPI;
  /** Agent Loop Registry API */
  agentLoopRegistry: AgentLoopRegistryAPI;
  /** Agent Loop Resource API (deprecated - use agentLoopRegistry instead) */
  /** @deprecated Use {@link agentLoopRegistry} instead */
  agentLoopResource: AgentLoopResourceAPI;
  /** Agent Loop Checkpoint API */
  agentLoopCheckpoints: AgentLoopCheckpointResourceAPI;
  /** Agent Loop Message API */
  agentLoopMessages: AgentLoopMessageResourceAPI;
  /** Agent Loop Iteration API */
  agentLoopIteration: AgentLoopIterationAPI;
  /** Agent Variable API */
  agentVariables: AgentVariableResourceAPI;
  /** Agent User Interaction API */
  agentUserInteractions: AgentUserInteractionResourceAPI;
  /** Agent Error Analysis API */
  agentErrorAnalysis: AgentErrorAnalysisAPI;
  /** Agent Performance Analysis API */
  agentPerformance: AgentPerformanceAnalysisAPI;
  /** Agent Execution Registry API */
  agentExecutionRegistry: AgentExecutionRegistryAPI;
  /** Agent Execution State API */
  agentExecutionState: AgentExecutionStateAPI;
  /** Agent Execution Graph API */
  agentExecutionGraph: AgentExecutionGraphQueryAPI;
  /** Agent Trigger API */
  agentTriggers: AgentTriggerResourceAPI;
  /** Agent Trigger Template API */
  agentTriggerTemplates: AgentTriggerTemplateRegistryAPI;
  /** Agent Hook Template API */
  agentHookTemplates: AgentHookTemplateRegistryAPI;
  /** Agent Template API */
  agentTemplates: AgentTemplateRegistryAPI;
}

/**
 * APIFactory class
 *
 * Usage example:
 * ```typescript
 * // Create factory instance with global context
 * const factory = new APIFactory(globalContext);
 *
 * // Create a single API
 * const workflowAPI = factory.createWorkflowAPI();
 *
 * // Create all APIs
 * const apis = factory.createAllAPIs();
 * ```
 */
export class APIFactory {
  private apiInstances: Partial<AllAPIs> = {};
  private dependencies: APIDependencyManager;

  /**
   * Create a new APIFactory instance
   * @param globalContext The GlobalContext to get dependencies from
   */
  constructor(globalContext: import("../../../shared/global-context.js").GlobalContext) {
    this.dependencies = new APIDependencyManager(globalContext);
  }

  /**
   * Reset the factory instance
   */
  public reset(): void {
    this.apiInstances = {};
  }

  /**
   * Generic method for creating or retrieving an API instance by key and constructor.
   *
   * This is the recommended way to create API instances when adding new APIs,
   * as it reduces boilerplate compared to adding a new createXxx method for each API.
   *
   * Usage:
   * ```typescript
   * const api = factory.getAPI("workflows", WorkflowRegistryAPI);
   * ```
   *
   * @param key The AllAPIs key to associate with this instance
   * @param APIConstructor The API constructor (requires APIDependencyManager)
   * @returns The API instance (cached if already created)
   */
  public getAPI<K extends keyof AllAPIs, T extends AllAPIs[K]>(
    key: K,
    APIConstructor: new (deps: APIDependencyManager) => T,
  ): T {
    return this.createAPI(key, APIConstructor);
  }

  /**
   * Generic method for creating or retrieving an API instance without dependencies.
   *
   * @param key The AllAPIs key to associate with this instance
   * @param APIConstructor The API constructor (no parameters required)
   * @returns The API instance (cached if already created)
   */
  public getAPIWithoutDeps<K extends keyof AllAPIs, T extends AllAPIs[K]>(
    key: K,
    APIConstructor: new () => T,
  ): T {
    return this.createAPIWithoutDeps(key, APIConstructor);
  }

  /**
   * General method for creating an API instance with consistent caching
   * @param key: The key name of the API instance
   * @param APIConstructor: The API constructor (requires dependencies parameter)
   * @returns: The API instance (cached if already created)
   */
  private createAPI<T extends AllAPIs[keyof AllAPIs]>(
    key: keyof AllAPIs,
    APIConstructor: new (deps: APIDependencyManager) => T,
  ): T {
    // Check cache first
    const cachedInstance = this.apiInstances[key];
    if (cachedInstance) {
      return cachedInstance as T;
    }

    // Create new instance and cache it
    const newInstance: T = new APIConstructor(this.dependencies);
    (this.apiInstances as Record<keyof AllAPIs, AllAPIs[keyof AllAPIs]>)[key] = newInstance;
    return newInstance;
  }

  /**
   * General method for creating API instances without dependencies with consistent caching
   * @param key: The key name of the API instance
   * @param APIConstructor: The API constructor (no parameters required)
   * @returns: The API instance (cached if already created)
   *
   * This method handles APIs that don't require dependencies from APIDependencyManager
   */
  private createAPIWithoutDeps<T extends AllAPIs[keyof AllAPIs]>(
    key: keyof AllAPIs,
    APIConstructor: new () => T,
  ): T {
    // Check cache first
    const cachedInstance = this.apiInstances[key];
    if (cachedInstance) {
      return cachedInstance as T;
    }

    // Create new instance and cache it
    const newInstance: T = new APIConstructor();
    (this.apiInstances as Record<keyof AllAPIs, AllAPIs[keyof AllAPIs]>)[key] = newInstance;
    return newInstance;
  }

  /**
   * Create a workflow API
   * @returns WorkflowRegistryAPI instance
   */
  public createWorkflowAPI(): WorkflowRegistryAPI {
    return this.createAPI("workflows", WorkflowRegistryAPI);
  }

  /**
   * Create a tool API
   * @returns ToolRegistryAPI instance
   */
  public createToolAPI(): ToolRegistryAPI {
    return this.createAPI("tools", ToolRegistryAPI);
  }

  /**
   * Create a workflow execution API
   * @returns WorkflowExecutionRegistryAPI instance
   */
  public createWorkflowExecutionAPI(): WorkflowExecutionRegistryAPI {
    return this.createAPI("executions", WorkflowExecutionRegistryAPI);
  }

  /**
   * Create a script API
   * @returns ScriptRegistryAPI instance
   */
  public createScriptAPI(): ScriptRegistryAPI {
    return this.createAPI("scripts", ScriptRegistryAPI);
  }

  /**
   * Create the Profile API
   * @returns ProfileRegistryAPI instance
   */
  public createProfileAPI(): LLMProfileRegistryAPI {
    return this.createAPI("profiles", LLMProfileRegistryAPI);
  }

  /**
   * Create a node template API
   * @returns NodeRegistryAPI instance
   */
  public createNodeTemplateAPI(): NodeRegistryAPI {
    return this.createAPI("nodeTemplates", NodeRegistryAPI);
  }

  /**
   * Create a trigger template API
   * @returns TriggerTemplateRegistryAPI instance
   */
  public createTriggerTemplateAPI(): TriggerTemplateRegistryAPI {
    return this.createAPI("triggerTemplates", TriggerTemplateRegistryAPI);
  }

  /**
   * Create a user interaction API
   * @returns UserInteractionResourceAPI instance
   */
  public createUserInteractionAPI(): UserInteractionResourceAPI {
    return this.createAPI("userInteractions", UserInteractionResourceAPI);
  }

  /**
   * Create an event API
   * @returns EventResourceAPI instance
   */
  public createEventAPI(): EventResourceAPI {
    return this.createAPI("events", EventResourceAPI);
  }

  /**
   * Create a trigger API
   * @returns TriggerResourceAPI instance
   */
  public createTriggerAPI(): TriggerResourceAPI {
    return this.createAPI("triggers", TriggerResourceAPI);
  }

  /**
   * Create a VariableAPI instance
   * @returns VariableResourceAPI instance
   */
  public createVariableAPI(): VariableResourceAPI {
    return this.createAPI("variables", VariableResourceAPI);
  }

  /**
   * Create a message API
   * @returns MessageResourceAPI instance
   */
  public createMessageAPI(): MessageResourceAPI {
    return this.createAPI("messages", MessageResourceAPI);
  }

  /**
   * Create a Skill API
   * @returns SkillRegistryAPI instance
   */
  public createSkillAPI(): SkillRegistryAPI {
    return this.createAPI("skills", SkillRegistryAPI);
  }

  /**
   * Create a Metrics API
   * @returns MetricsResourceAPI instance
   */
  public createMetricsAPI(): MetricsResourceAPI {
    return this.createAPI("metrics", MetricsResourceAPI);
  }

  /**
   * Create a Task API
   * @returns TaskResourceAPI instance
   */
  public createTaskAPI(): TaskResourceAPI {
    return this.createAPI("tasks", TaskResourceAPI);
  }

  /**
   * Create a Workflow Graph Query API
   * @returns WorkflowGraphQueryAPI instance
   */
  public createWorkflowGraphQueryAPI(): WorkflowGraphQueryAPI {
    return this.createAPI("graphs", WorkflowGraphQueryAPI);
  }

  /**
   * Create a Storage Diagnostics API
   * @returns StorageDiagnosticsAPI instance
   */
  public createStorageDiagnosticsAPI(): StorageDiagnosticsAPI {
    return this.createAPI("diagnostics", StorageDiagnosticsAPI);
  }

  /**
   * Create a Search API
   * @returns SearchAPI instance
   */
  public createSearchAPI(): SearchAPI {
    return this.createAPI("search", SearchAPI);
  }

  /**
   * Create a File Checkpoint API
   * @returns FileCheckpointResourceAPI instance
   */
  public createFileCheckpointAPI(): FileCheckpointResourceAPI {
    return this.createAPI("fileCheckpoints", FileCheckpointResourceAPI);
  }

  /**
   * Create an Agent Loop Registry API
   * @returns AgentLoopRegistryAPI instance
   */
  public createAgentLoopRegistryAPI(): AgentLoopRegistryAPI {
    return this.createAPI("agentLoopRegistry", AgentLoopRegistryAPI);
  }

  /**
   * Create an Agent Loop Resource API
   *
   * @deprecated Use {@link createAgentLoopRegistryAPI} instead. AgentLoopRegistryAPI now
   * provides all state management methods previously only available in AgentLoopResourceAPI.
   * This method will be removed in a future major version.
   *
   * @returns AgentLoopResourceAPI instance
   *
   * Note: This API doesn't require dependencies, using createAPIWithoutDeps for consistency
   */
  public createAgentLoopResourceAPI(): AgentLoopResourceAPI {
    return this.createAPIWithoutDeps("agentLoopResource", AgentLoopResourceAPI);
  }

  /**
   * Create an Agent Loop Checkpoint API
   * @returns AgentLoopCheckpointResourceAPI instance
   *
   * Note: This API doesn't require dependencies, using createAPIWithoutDeps for consistency
   */
  public createAgentLoopCheckpointAPI(): AgentLoopCheckpointResourceAPI {
    return this.createAPIWithoutDeps("agentLoopCheckpoints", AgentLoopCheckpointResourceAPI);
  }

  /**
   * Create an Agent Loop Message API
   * @returns AgentLoopMessageResourceAPI instance
   */
  public createAgentLoopMessageAPI(): AgentLoopMessageResourceAPI {
    return this.createAPI("agentLoopMessages", AgentLoopMessageResourceAPI);
  }

  /**
   * Create an Agent Loop Iteration API
   * @returns AgentLoopIterationAPI instance
   */
  public createAgentLoopIterationAPI(): AgentLoopIterationAPI {
    return this.createAPI("agentLoopIteration", AgentLoopIterationAPI);
  }

  /**
   * Create an Agent Variable API
   * @returns AgentVariableResourceAPI instance
   */
  public createAgentVariableAPI(): AgentVariableResourceAPI {
    return this.createAPI("agentVariables", AgentVariableResourceAPI);
  }

  /**
   * Create an Agent User Interaction API
   * @returns AgentUserInteractionResourceAPI instance
   *
   * Note: This API doesn't require dependencies, using createAPIWithoutDeps for consistency
   */
  public createAgentUserInteractionAPI(): AgentUserInteractionResourceAPI {
    return this.createAPIWithoutDeps("agentUserInteractions", AgentUserInteractionResourceAPI);
  }

  /**
   * Create an Agent Error Analysis API
   * @returns AgentErrorAnalysisAPI instance
   */
  public createAgentErrorAnalysisAPI(): AgentErrorAnalysisAPI {
    return this.createAPI("agentErrorAnalysis", AgentErrorAnalysisAPI);
  }

  /**
   * Create an Agent Performance Analysis API
   * @returns AgentPerformanceAnalysisAPI instance
   */
  public createAgentPerformanceAnalysisAPI(): AgentPerformanceAnalysisAPI {
    return this.createAPI("agentPerformance", AgentPerformanceAnalysisAPI);
  }

  /**
   * Create an Agent Execution Registry API
   * @returns AgentExecutionRegistryAPI instance
   */
  public createAgentExecutionRegistryAPI(): AgentExecutionRegistryAPI {
    return this.createAPI("agentExecutionRegistry", AgentExecutionRegistryAPI);
  }

  /**
   * Create an Agent Execution State API
   * @returns AgentExecutionStateAPI instance
   */
  public createAgentExecutionStateAPI(): AgentExecutionStateAPI {
    return this.createAPI("agentExecutionState", AgentExecutionStateAPI);
  }

  /**
   * Create an Agent Execution Graph Query API
   * @returns AgentExecutionGraphQueryAPI instance
   */
  public createAgentExecutionGraphQueryAPI(): AgentExecutionGraphQueryAPI {
    return this.createAPI("agentExecutionGraph", AgentExecutionGraphQueryAPI);
  }

  /**
   * Create an Agent Trigger API
   * @returns AgentTriggerResourceAPI instance
   */
  public createAgentTriggerResourceAPI(): AgentTriggerResourceAPI {
    return this.createAPI("agentTriggers", AgentTriggerResourceAPI);
  }

  /**
   * Create an Agent Trigger Template Registry API
   * @returns AgentTriggerTemplateRegistryAPI instance
   */
  public createAgentTriggerTemplateRegistryAPI(): AgentTriggerTemplateRegistryAPI {
    return this.createAPI("agentTriggerTemplates", AgentTriggerTemplateRegistryAPI);
  }

  /**
   * Create an Agent Hook Template Registry API
   * @returns AgentHookTemplateRegistryAPI instance
   */
  public createAgentHookTemplateRegistryAPI(): AgentHookTemplateRegistryAPI {
    return this.createAPI("agentHookTemplates", AgentHookTemplateRegistryAPI);
  }

  /**
   * Create an Agent Template Registry API
   * @returns AgentTemplateRegistryAPI instance
   */
  public createAgentTemplateRegistryAPI(): AgentTemplateRegistryAPI {
    return this.createAPI("agentTemplates", AgentTemplateRegistryAPI);
  }

  /**
   * Create all API instances and initialize event-driven systems
   * @returns All API instances
   */
  public createAllAPIs(): AllAPIs {
    const apis = {
      workflows: this.createWorkflowAPI(),
      tools: this.createToolAPI(),
      executions: this.createWorkflowExecutionAPI(),
      scripts: this.createScriptAPI(),
      profiles: this.createProfileAPI(),
      nodeTemplates: this.createNodeTemplateAPI(),
      triggerTemplates: this.createTriggerTemplateAPI(),
      userInteractions: this.createUserInteractionAPI(),
      events: this.createEventAPI(),
      triggers: this.createTriggerAPI(),
      variables: this.createVariableAPI(),
      messages: this.createMessageAPI(),
      skills: this.createSkillAPI(),
      metrics: this.createMetricsAPI(),
      tasks: this.createTaskAPI(),
      graphs: this.createWorkflowGraphQueryAPI(),
      diagnostics: this.createStorageDiagnosticsAPI(),
      search: this.createSearchAPI(),
      fileCheckpoints: this.createFileCheckpointAPI(),
      agentLoopRegistry: this.createAgentLoopRegistryAPI(),
      agentLoopResource: this.createAgentLoopResourceAPI(),
      agentLoopCheckpoints: this.createAgentLoopCheckpointAPI(),
      agentLoopMessages: this.createAgentLoopMessageAPI(),
      agentLoopIteration: this.createAgentLoopIterationAPI(),
      agentVariables: this.createAgentVariableAPI(),
      agentUserInteractions: this.createAgentUserInteractionAPI(),
      agentErrorAnalysis: this.createAgentErrorAnalysisAPI(),
      agentPerformance: this.createAgentPerformanceAnalysisAPI(),
      agentExecutionRegistry: this.createAgentExecutionRegistryAPI(),
      agentExecutionState: this.createAgentExecutionStateAPI(),
      agentExecutionGraph: this.createAgentExecutionGraphQueryAPI(),
      agentTriggers: this.createAgentTriggerResourceAPI(),
      agentTriggerTemplates: this.createAgentTriggerTemplateRegistryAPI(),
      agentHookTemplates: this.createAgentHookTemplateRegistryAPI(),
      agentTemplates: this.createAgentTemplateRegistryAPI(),
    };

    // Initialize event-driven systems: metrics collection and logging
    try {
      const metricsRegistry = this.dependencies.getGlobalContext().metricsRegistry;
      if (metricsRegistry && typeof metricsRegistry.subscribeToEvents === 'function') {
        metricsRegistry.subscribeToEvents();
      }

      const eventLogger = new ExecutionEventLogger();
      eventLogger.subscribe();
    } catch (error) {
      // If initialization fails, log but don't crash
      console.error("Failed to initialize event-driven systems", error);
    }

    return apis;
  }

  /**
   * Get the dependency manager for command execution
   * This method provides access to dependencies needed by commands
   *
   * @returns APIDependencyManager instance
   */
  getDependencies(): APIDependencyManager {
    return this.dependencies;
  }
}
