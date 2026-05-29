/**
 * DI Container Configuration
 * Configure all service bindings in the DI container and define the dependencies between services.
 *
 * Design Principles:
 * - Configure service bindings in the order of dependencies to avoid circular dependencies.
 * - Use the singleton lifecycle for stateless global services (such as Registry, Manager).
 * - Use the factory pattern for execution-isolated services (such as GraphConversationSession, ConversationSession).
 * - Factory functions are used to create instances that require runtime parameters (such as executionId, nodeId).
 *
 * Service Layering:
 * - First Layer: Dependency-free storage layer services (WorkflowGraphRegistry, WorkflowExecutionRegistry).
 * - Second Layer: Dependency-free business layer services (EventRegistry, ToolRegistry, etc.).
 * - Third Layer: Business layer services that depend on the second layer (ErrorService).
 * - Fourth Layer: Business layer services that depend on the storage layer (WorkflowRegistry).
 * - Fifth Layer: Basic execution layer services (LLMExecutor, ToolCallExecutor).
 * - Sixth Layer: Execution layer services that depend on the fifth layer (CheckpointState).
 * - Seventh Layer: WorkflowExecutor (depends on the WorkflowGraphRegistry and WorkflowExecutionCoordinator factories).
 * - Eighth Layer: WorkflowLifecycleCoordinator (using the factory pattern).
 * - Ninth Layer: WorkflowExecutionBuilder (no dependencies).
 * - Tenth Layer: Basic execution layer Managers (some use the factory pattern).
 * - Eleventh Layer: Execution layer Coordinators (some use the factory pattern).
 * - Twelfth Layer: Execution layer Coordinators (medium to low priority).
 * - Thirteenth Layer: WorkflowExecutionPool (must be bound last as it depends on WorkflowExecutor).
 */

import { Container, type IContainer } from "@wf-agent/common-utils";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  TaskStorageAdapter,
  WorkflowExecutionStorageAdapter,
  AgentLoopStorageAdapter,
  MetricsStorageAdapter,
  FileCheckpointStorageAdapter as FileCheckpointStorageAdapterType,
} from "@wf-agent/storage";
import type { FileCheckpointManager, FileCheckpointManagerConfig } from "@wf-agent/common-utils";
import type { GlobalContext } from "../global-context.js";
import * as Identifiers from "./service-identifiers.js";
import type {
  IdBasedServiceFactory,
  NoArgServiceFactory,
  OptionalParamsServiceFactory,
  ExecutionEntityServiceFactory,
  NodeExecutionCoordinatorFactory,
  InterruptionStateFactory,
} from "./factory-types.js";

// Storage Layer Service
import { WorkflowGraphRegistry } from "../../workflow/stores/workflow-graph-registry.js";
import { WorkflowExecutionRegistry } from "../../workflow/stores/workflow-execution-registry.js";
import { LLMWrapper } from "../llm/wrapper.js";

// Business Layer Services
import { EventRegistry } from "../registry/event-registry.js";
import { ToolRegistry } from "../registry/tool-registry.js";
import { ScriptRegistry } from "../registry/script-registry.js";
import { NodeTemplateRegistry } from "../registry/node-template-registry.js";
import { TriggerTemplateRegistry } from "../registry/trigger-template-registry.js";
import { TimeoutRegistry } from "../registry/timeout-registry.js";

import { TaskRegistry } from "../../workflow/stores/task/task-registry.js";
import { TaskQueue } from "../../workflow/stores/task/task-queue.js";
import { WorkflowRegistry } from "../../workflow/stores/workflow-registry.js";
import { WorkflowRelationshipRegistry } from "../../workflow/stores/workflow-relationship-registry.js";
import { WorkflowExecutionPool } from "../../workflow/execution/workflow-execution-pool.js";

// Execution Layer Services - Core Layer Universal Executor
import { LLMExecutor, ToolCallExecutor } from "../executors/index.js";
import { ToolApprovalCoordinator } from "../coordinators/tool-approval-coordinator.js";
import { SkillRegistry } from "../registry/skill-registry.js";
import { SkillLoader } from "../utils/skill-loader.js";
import { emit } from "../../workflow/execution/utils/index.js";
import { CheckpointCoordinator } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import {
  buildMessageAddedEvent,
  buildToolCallStartedEvent,
  buildToolCallCompletedEvent,
  buildToolCallFailedEvent,
} from "../../workflow/execution/utils/event/index.js";
import { WorkflowStateTransitor } from "../../workflow/execution/coordinators/workflow-state-transitor.js";
import { CheckpointState } from "../../workflow/checkpoint/checkpoint-state-manager.js";
import { WorkflowExecutionBuilder } from "../../workflow/execution/factories/workflow-execution-builder.js";
import { WorkflowExecutor } from "../../workflow/execution/executors/workflow-executor.js";
import { ToolPermissionManager } from "../coordinators/tool-permission-manager.js";
import { RejectionMessageBuilder } from "../coordinators/rejection-message-builder.js";

// Execution Layer - Coordinators
import { WorkflowExecutionCoordinator } from "../../workflow/execution/coordinators/workflow-execution-coordinator.js";
import { VariableCoordinator } from "../../workflow/execution/coordinators/variable-coordinator.js";
import { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import { NodeExecutionCoordinator } from "../../workflow/execution/coordinators/node-execution-coordinator.js";
import { TriggeredSubworkflowHandler } from "../../workflow/execution/handlers/triggered-subworkflow-handler.js";
import { LLMExecutionCoordinator } from "../../workflow/execution/coordinators/llm-execution-coordinator.js";
import { WorkflowNavigator } from "../../workflow/builder/workflow-navigator.js";
import { WorkflowLifecycleCoordinator } from "../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js";

// Execution Layer - Managers
import { ConversationSession } from "../messaging/conversation-session.js";
import { VariableManager } from "../../workflow/state-managers/variable-manager.js";
import { TriggerState } from "../../workflow/state-managers/trigger-state.js";
import { InterruptionState } from "../utils/interruption/interruption-state.js";
import { AgentLoopExecutor } from "../../agent/execution/executors/agent-loop-executor.js";
import { AgentLoopRegistry } from "../../agent/stores/agent-loop-registry.js";
import type { IAgentExecutionRegistry } from "../../agent/stores/agent-execution-registry.js";
import { ExecutionHierarchyRegistry } from "../registry/execution-hierarchy-registry.js";
import { AgentLoopCoordinator } from "../../agent/execution/coordinators/agent-loop-coordinator.js";
import { WorkflowExecutionEntity } from "../../workflow/entities/workflow-execution-entity.js";
import { MetricsRegistry } from "../metrics/metrics-registry.js";
import type { MetricsConfig, TimeoutConfig } from "@wf-agent/types";
import type { SDKOptions } from "../../api/shared/types/core-types.js";
import {
  loadMetricsConfigFromFile,
  mergeMetricsWithDefaults,
  getMetricsEnvironmentDefaults,
  loadTimeoutConfigFromFile,
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
} from "../../api/shared/config/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ContainerConfig" });

/**
 * Storage adapter configuration for container initialization
 */
export interface ContainerStorageConfig {
  checkpoint?: CheckpointStorageAdapter;
  workflow?: WorkflowStorageAdapter;
  task?: TaskStorageAdapter;
  workflowExecution?: WorkflowExecutionStorageAdapter;
  agentLoop?: AgentLoopStorageAdapter;
  fileCheckpointStorageAdapter?: FileCheckpointStorageAdapterType;
  fileCheckpointManagerConfig?: FileCheckpointManagerConfig;
}

/**
 * Configure container bindings with storage adapters
 * This function configures all service bindings for a given container
 *
 * @param container The container to configure
 * @param adapters All storage adapters (optional)
 */
export function configureContainerBindings(
  container: Container,
  adapters: ContainerStorageConfig = {},
): void {
  // ============================================================
  // Bind Storage Adapters in DI Container
  // ============================================================

  // CheckpointStorageAdapter
  container
    .bind(Identifiers.CheckpointStorageAdapter)
    .toDynamicValue(() => adapters.checkpoint || null)
    .inSingletonScope();

  // WorkflowStorageAdapter
  container
    .bind(Identifiers.WorkflowStorageAdapter)
    .toDynamicValue(() => adapters.workflow || null)
    .inSingletonScope();

  // TaskStorageAdapter
  container
    .bind(Identifiers.TaskStorageAdapter)
    .toDynamicValue(() => adapters.task || null)
    .inSingletonScope();

  // WorkflowExecutionStorageAdapter
  container
    .bind(Identifiers.WorkflowExecutionStorageAdapter)
    .toDynamicValue(() => adapters.workflowExecution || null)
    .inSingletonScope();

  // AgentLoopStorageAdapter
  container
    .bind(Identifiers.AgentLoopStorageAdapter)
    .toDynamicValue(() => adapters.agentLoop || null)
    .inSingletonScope();

  // FileCheckpointStorageAdapter (optional, only bound when file checkpoint is enabled)
  if (adapters.fileCheckpointStorageAdapter) {
    container
      .bind(Identifiers.FileCheckpointStorageAdapter)
      .toDynamicValue(() => adapters.fileCheckpointStorageAdapter)
      .inSingletonScope();
  } else {
    container
      .bind(Identifiers.FileCheckpointStorageAdapter)
      .toDynamicValue(() => null)
      .inSingletonScope();
  }

  // FileCheckpointManager (dynamic, created when adapter is provided and config is valid)
  if (adapters.fileCheckpointStorageAdapter && adapters.fileCheckpointManagerConfig?.enabled) {
    container
      .bind(Identifiers.FileCheckpointManager)
      .toDynamicValue(async () => {
        const { FileCheckpointManager: FCM } = await import("@wf-agent/common-utils");
        const fcm = new FCM(
          adapters.fileCheckpointStorageAdapter!,
          adapters.fileCheckpointManagerConfig!,
        );
        return fcm;
      })
      .inSingletonScope();
  } else {
    container
      .bind(Identifiers.FileCheckpointManager)
      .toDynamicValue(() => undefined)
      .inSingletonScope();
  }

  // ============================================================
  // First Layer: A storage layer service with no dependencies
  // ============================================================

  container.bind(Identifiers.WorkflowGraphRegistry).to(WorkflowGraphRegistry).inSingletonScope();

  // WorkflowExecutionRegistry - Depends on WorkflowExecutionStorageAdapter for persistence
  container
    .bind(Identifiers.WorkflowExecutionRegistry)
    .toDynamicValue((c: IContainer): WorkflowExecutionRegistry => {
      const storageAdapter = c.get(
        Identifiers.WorkflowExecutionStorageAdapter,
      ) as WorkflowExecutionStorageAdapter | null;

      return new WorkflowExecutionRegistry({
        storageAdapter: storageAdapter || undefined,
      });
    })
    .inSingletonScope();

  // LLMWrapper - An LLM wrapper that relies on EventRegistry for event publishing.
  container
    .bind(Identifiers.LLMWrapper)
    .toDynamicValue(
      (c: IContainer) => new LLMWrapper(c.get(Identifiers.EventRegistry) as EventRegistry),
    )
    .inSingletonScope();

  // ============================================================
  // Level 2: Business layer services with no dependencies
  // ============================================================

  container
    .bind(Identifiers.EventRegistry)
    .toDynamicValue(() => new EventRegistry())
    .inSingletonScope();

  container
    .bind(Identifiers.ToolRegistry)
    .toDynamicValue(() => new ToolRegistry())
    .inSingletonScope();

  container.bind(Identifiers.ScriptRegistry).to(ScriptRegistry).inSingletonScope();

  container.bind(Identifiers.NodeTemplateRegistry).to(NodeTemplateRegistry).inSingletonScope();

  container
    .bind(Identifiers.TriggerTemplateRegistry)
    .to(TriggerTemplateRegistry)
    .inSingletonScope();

  // TimeoutRegistry - Timeout registry with optional configuration
  container
    .bind(Identifiers.TimeoutRegistry)
    .toDynamicValue(() => new TimeoutRegistry())
    .inSingletonScope();

  // TaskRegistry - Task registry (per-SDK-instance singleton)
  // Each SDK instance gets its own TaskRegistry to ensure isolation
  // Note: Async initialization (loading from storage) is handled in SDKInstance.bootstrap()
  container
    .bind(Identifiers.TaskRegistry)
    .toDynamicValue((c: IContainer) => {
      const storageAdapter = c.get(Identifiers.TaskStorageAdapter) as TaskStorageAdapter | null;
      return new TaskRegistry({
        storageAdapter: storageAdapter || undefined,
        autoPersist: true,
      });
    })
    .inSingletonScope();

  // ============================================================
  // Skill Layer Service
  // ============================================================

  // SkillRegistry - Skill registry, path needs to be configured
  // Use the factory pattern to allow the application layer to provide configuration
  // The default configuration uses an empty path list; the application layer should rebind this service and provide the actual configuration at initialization time
  container
    .bind(Identifiers.SkillRegistry)
    .toDynamicValue(() => {
      const config = {
        paths: [],
        autoScan: true,
        cacheEnabled: true,
        cacheTTL: 300000,
      };
      return new SkillRegistry(config);
    })
    .inSingletonScope();

  // SkillLoader - A Skill loader that relies on SkillRegistry and EventRegistry
  container
    .bind(Identifiers.SkillLoader)
    .toDynamicValue((c: IContainer): SkillLoader => {
      return new SkillLoader(
        c.get(Identifiers.SkillRegistry) as SkillRegistry,
        c.get(Identifiers.EventRegistry) as EventRegistry,
      );
    })
    .inSingletonScope();

  // ============================================================
  // Layer 4: Business layer services that rely on the storage layer
  // ============================================================

  // WorkflowRelationshipRegistry - A relationship registry with no dependencies
  container
    .bind(Identifiers.WorkflowRelationshipRegistry)
    .to(WorkflowRelationshipRegistry)
    .inSingletonScope();

  // WorkflowRegistry - Receives dependencies directly, no longer depends on GlobalContext
  // All dependencies are registered before WorkflowRegistry, so there are no container-level circular dependencies
  container
    .bind(Identifiers.WorkflowRegistry)
    .toDynamicValue((c: IContainer): WorkflowRegistry => {
      const storageAdapter = c.get(
        Identifiers.WorkflowStorageAdapter,
      ) as WorkflowStorageAdapter | null;
      const relationshipRegistry = c.get(
        Identifiers.WorkflowRelationshipRegistry,
      ) as WorkflowRelationshipRegistry;
      const executionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as
        | WorkflowExecutionRegistry
        | undefined;
      const graphRegistry = c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;

      return new WorkflowRegistry(
        storageAdapter,
        executionRegistry,
        relationshipRegistry,
        graphRegistry,
      );
    })
    .inSingletonScope();

  // ============================================================
  // Layer 5: Basic Services of the Execution Layer
  // ============================================================

  // LLMExecutor - The LLM executor, which relies on LLMWrapper
  container
    .bind(Identifiers.LLMExecutor)
    .toDynamicValue((c: IContainer): LLMExecutor => {
      const llmWrapper = c.get(Identifiers.LLMWrapper) as LLMWrapper;
      return new LLMExecutor(llmWrapper);
    })
    .inSingletonScope();

  // ToolCallExecutor - A tool call executor that relies on multiple services and coordinators.
  container
    .bind(Identifiers.ToolCallExecutor)
    .toDynamicValue((c: IContainer): ToolCallExecutor => {
      // The Graph module features a dedicated event builder.
      const eventBuilder = {
        buildMessageAddedEvent,
        buildToolCallStartedEvent,
        buildToolCallCompletedEvent,
        buildToolCallFailedEvent,
      };

      return new ToolCallExecutor(c.get(Identifiers.ToolRegistry) as ToolRegistry, {
        eventManager: c.get(Identifiers.EventRegistry) as EventRegistry,
        checkpointDependencies: {
          workflowExecutionRegistry: c.get(
            Identifiers.WorkflowExecutionRegistry,
          ) as WorkflowExecutionRegistry,
          checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
          workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
          workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
        },
        eventBuilder,
        createCheckpointFn: (options, deps) =>
          CheckpointCoordinator.createCheckpoint(options.workflowExecutionId, deps, {
            description: options.description,
            ...(options.toolId ? { customFields: { toolId: options.toolId } } : {}),
          }),
        safeEmitFn: emit,
      });
    })
    .inSingletonScope();

  // ToolApprovalCoordinator - A tool approval coordinator that relies on the EventRegistry.
  container
    .bind(Identifiers.ToolApprovalCoordinator)
    .toDynamicValue((c: IContainer): ToolApprovalCoordinator => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      return new ToolApprovalCoordinator(eventManager);
    })
    .inSingletonScope();

  // ConversationSession Factory - Execution-isolated conversation session
  // Each workflow execution requires a separate instance of the conversation session.
  // Create instances using the factory pattern to ensure data isolation between executions
  container
    .bind(Identifiers.GraphConversationSession)
    .toDynamicValue((): IdBasedServiceFactory<ConversationSession> => {
      const factory: IdBasedServiceFactory<ConversationSession> = {
        create: (executionId: string) => new ConversationSession({ executionId }),
      };
      return factory;
    })
    .inSingletonScope();

  // WorkflowStateTransitor - Workflow State Transitor Factory
  // Each workflow execution requires a separate instance of the state transitor
  container
    .bind(Identifiers.WorkflowStateTransitor)
    .toDynamicValue((c: IContainer): IdBasedServiceFactory<WorkflowStateTransitor> => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const workflowConversationSessionFactory = c.get(Identifiers.GraphConversationSession);
      const workflowExecutionRegistry = c.get(
        Identifiers.WorkflowExecutionRegistry,
      ) as WorkflowExecutionRegistry;

      const factory: IdBasedServiceFactory<WorkflowStateTransitor> = {
        create: (executionId: string) => {
          const workflowConversationSession = (
            workflowConversationSessionFactory as unknown as IdBasedServiceFactory<ConversationSession>
          ).create(executionId);
          const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
          return new WorkflowStateTransitor(
            eventManager,
            workflowConversationSession,
            workflowExecutionRegistry,
            globalContext,
          );
        },
      };
      return factory;
    })
    .inSingletonScope();

  // ============================================================
  // Layer six: Depends on the execution layer services of layer five
  // ============================================================

  // CheckpointState - Checkpoint State Manager
  // Requires CheckpointStorageAdapter from DI container
  container
    .bind(Identifiers.CheckpointState)
    .toDynamicValue((c: IContainer): CheckpointState => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const adapter = c.get(Identifiers.CheckpointStorageAdapter) as CheckpointStorageAdapter;

      if (!adapter) {
        throw new Error(
          "CheckpointState requires a CheckpointStorageAdapter implementation. " +
            "Please provide it via initializeContainerWithAdapters({ checkpoint: adapter }) parameter.",
        );
      }

      return new CheckpointState(adapter, eventManager);
    })
    .inSingletonScope();

  // ============================================================
  // Layer 7: WorkflowExecutor (depends on WorkflowGraphRegistry and WorkflowExecutionCoordinator factories)
  // ============================================================

  // WorkflowExecutor - A workflow executor that relies on the WorkflowGraphRegistry and WorkflowExecutionCoordinator factories.
  container
    .bind(Identifiers.WorkflowExecutor)
    .toDynamicValue((c: IContainer): WorkflowExecutor => {
      // The WorkflowExecutionCoordinator identifier is bound to a factory object with a create method
      const coordinatorFactory = c.get(Identifiers.WorkflowExecutionCoordinator) as unknown as {
        create(
          executionEntity: import("../../workflow/entities/workflow-execution-entity.js").WorkflowExecutionEntity,
        ): import("../../workflow/execution/coordinators/workflow-execution-coordinator.js").WorkflowExecutionCoordinator;
      };
      return new WorkflowExecutor({
        workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
        workflowExecutionCoordinatorFactory: coordinatorFactory,
      });
    })
    .inSingletonScope();

  // ============================================================
  // Layer 8: WorkflowLifecycleCoordinator (Factory Pattern)
  // ============================================================

  // WorkflowLifecycleCoordinator - Workflow Lifecycle Coordinator Factory
  // Each workflow execution requires a separate instance of the lifecycle coordinator
  // This binds the high-level WorkflowLifecycleCoordinator from execution/coordinators
  // which depends on WorkflowStateTransitor, WorkflowExecutionBuilder, WorkflowExecutor
  container
    .bind(Identifiers.WorkflowLifecycleCoordinator)
    .toDynamicValue((c: IContainer): IdBasedServiceFactory<WorkflowLifecycleCoordinator> => {
      const workflowExecutionRegistry = c.get(
        Identifiers.WorkflowExecutionRegistry,
      ) as WorkflowExecutionRegistry;
      const workflowExecutionBuilder = c.get(
        Identifiers.WorkflowExecutionBuilder,
      ) as WorkflowExecutionBuilder;
      const workflowExecutor = c.get(Identifiers.WorkflowExecutor) as WorkflowExecutor;
      const stateTransitorFactory = c.get(Identifiers.WorkflowStateTransitor);

      const factory: IdBasedServiceFactory<WorkflowLifecycleCoordinator> = {
        create: (executionId: string) => {
          const stateTransitor = (
            stateTransitorFactory as unknown as IdBasedServiceFactory<WorkflowStateTransitor>
          ).create(executionId);
          const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
          return new WorkflowLifecycleCoordinator(
            workflowExecutionRegistry,
            stateTransitor,
            workflowExecutionBuilder,
            workflowExecutor,
            globalContext,
          );
        },
      };
      return factory;
    })
    .inSingletonScope();

  // ============================================================
  // Layer 9: WorkflowExecutionBuilder (depends on GlobalContext)
  // ============================================================

  // WorkflowExecutionBuilder - A workflow execution builder that depends on GlobalContext
  container
    .bind(Identifiers.WorkflowExecutionBuilder)
    .toDynamicValue(c => {
      const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
      return new WorkflowExecutionBuilder(globalContext);
    })
    .inSingletonScope();

  // ============================================================
  // Level 10: Basic Managers at the Execution Layer (without dependencies or factory patterns)
  // ============================================================

  // VariableManager - Simplified Variable State Manager Factory (NEW)
  // VariableManager requires executionId, use factory pattern to create instance
  container
    .bind(Identifiers.VariableManager)
    .toDynamicValue(() => {
      return {
        create: () => new VariableManager(),
      };
    })
    .inSingletonScope();

  // TriggerState - Trigger State Manager Factory
  // TriggerState requires executionId, use factory pattern to create instance
  container
    .bind(Identifiers.TriggerState)
    .toDynamicValue((): IdBasedServiceFactory<TriggerState> => {
      const factory: IdBasedServiceFactory<TriggerState> = {
        create: (executionId: string) => new TriggerState(executionId),
      };
      return factory;
    })
    .inSingletonScope();

  // InterruptionState - Interruption Manager Factory
  // InterruptionState requires executionId and nodeId, and uses the factory pattern to create instances.
  container
    .bind(Identifiers.InterruptionState)
    .toDynamicValue((): InterruptionStateFactory<InterruptionState> => {
      const factory: InterruptionStateFactory<InterruptionState> = {
        create: (executionId: string, nodeId: string) =>
          new InterruptionState({ contextId: executionId, nodeId }),
      };
      return factory;
    })
    .inSingletonScope();

  // ============================================================
  // Layer Eleven: Execution Layer Coordinators (High Priority)
  // ============================================================

  // VariableCoordinator - A stateless variable coordinator
  // Note: This coordinator receives VariableManager as a parameter in each method call,
  // ensuring it always operates on the correct instance without maintaining state.
  container
    .bind(Identifiers.VariableCoordinator)
    .toDynamicValue((c: IContainer): VariableCoordinator => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      return new VariableCoordinator(eventManager);
    })
    .inSingletonScope();

  // LLMExecutionCoordinator - LLM execution coordinator, depends on LLMExecutor, ToolRegistry, EventRegistry, ToolCallExecutor
  container
    .bind(Identifiers.LLMExecutionCoordinator)
    .toDynamicValue((c: IContainer): LLMExecutionCoordinator => {
      const llmExecutor = c.get(Identifiers.LLMExecutor) as LLMExecutor;
      const toolService = c.get(Identifiers.ToolRegistry) as ToolRegistry;
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const toolCallExecutor = c.get(Identifiers.ToolCallExecutor) as ToolCallExecutor;
      return new LLMExecutionCoordinator({
        llmExecutor,
        toolService,
        eventManager,
        toolCallExecutor,
      });
    })
    .inSingletonScope();

  // ConversationSession - Conversation Manager Factory
  // ConversationSession is stateful and requires a separate instance for each execution to achieve execution isolation
  // Creating instances using the factory pattern ensures that the session data for each execution is independent of each other
  container
    .bind(Identifiers.ConversationSession)
    .toDynamicValue((c: IContainer): OptionalParamsServiceFactory<ConversationSession> => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;

      const factory: OptionalParamsServiceFactory<ConversationSession> = {
        create: (executionId?: string, workflowId?: string) => {
          return new ConversationSession({
            eventManager,
            executionId,
            workflowId,
          });
        },
      };
      return factory;
    })
    .inSingletonScope();

  // NodeExecutionCoordinator - dependency on multiple services and coordinators
  // NodeExecutionCoordinator - node execution coordinator factory
  // Each workflow execution requires a separate node execution coordinator instance
  container
    .bind(Identifiers.NodeExecutionCoordinator)
    .toDynamicValue((c: IContainer) => {
      const conversationManagerFactory = c.get(Identifiers.ConversationSession);
      const interruptionManagerFactory = c.get(Identifiers.InterruptionState);
      const agentLoopExecutorFactory = c.get(Identifiers.AgentLoopExecutor);

      const factory: NodeExecutionCoordinatorFactory<NodeExecutionCoordinator> = {
        create: (executionId: string, nodeId: string, executionEntity: WorkflowExecutionEntity) => {
          const graph = executionEntity.getGraph();
          const navigator = new WorkflowNavigator(graph);
          const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
          const config = {
            globalContext,
            eventManager: c.get(Identifiers.EventRegistry) as EventRegistry,
            llmCoordinator: c.get(Identifiers.LLMExecutionCoordinator) as LLMExecutionCoordinator,
            llmWrapper: c.get(Identifiers.LLMWrapper) as LLMWrapper,
            conversationManager: (
              conversationManagerFactory as unknown as OptionalParamsServiceFactory<ConversationSession>
            ).create(executionId),
            interruptionManager: (
              interruptionManagerFactory as unknown as InterruptionStateFactory<InterruptionState>
            ).create(executionId, nodeId),
            navigator,
            toolService: c.get(Identifiers.ToolRegistry) as ToolRegistry,
            permissionManager: c.get(
              Identifiers.ToolPermissionManager,
            ) as ToolPermissionManager | null,
            rejectionBuilder: c.get(Identifiers.RejectionMessageBuilder) as RejectionMessageBuilder,
            checkpointDependencies: {
              workflowExecutionRegistry: c.get(
                Identifiers.WorkflowExecutionRegistry,
              ) as WorkflowExecutionRegistry,
              checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
              workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
              workflowGraphRegistry: c.get(
                Identifiers.WorkflowGraphRegistry,
              ) as WorkflowGraphRegistry,
            },
            agentLoopExecutorFactory: (
              agentLoopExecutorFactory as unknown as NoArgServiceFactory<AgentLoopExecutor>
            ).create(),
            executionBuilder: c.get(
              Identifiers.WorkflowExecutionBuilder,
            ) as WorkflowExecutionBuilder,
            workflowExecutor: c.get(Identifiers.WorkflowExecutor) as WorkflowExecutor,
          };
          return new NodeExecutionCoordinator(config);
        },
      };
      return factory;
    })
    .inSingletonScope();

  // TriggerCoordinator - Dependencies on multiple services and coordinators
  // TriggerCoordinator - Trigger Coordinator Factory
  // Each workflow execution requires a separate instance of the trigger coordinator
  container
    .bind(Identifiers.TriggerCoordinator)
    .toDynamicValue((c: IContainer): IdBasedServiceFactory<TriggerCoordinator> => {
      const workflowExecutionRegistry = c.get(
        Identifiers.WorkflowExecutionRegistry,
      ) as WorkflowExecutionRegistry;
      const workflowRegistry = c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry;
      const workflowGraphRegistry = c.get(
        Identifiers.WorkflowGraphRegistry,
      ) as WorkflowGraphRegistry;
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const workflowExecutionBuilder = c.get(
        Identifiers.WorkflowExecutionBuilder,
      ) as WorkflowExecutionBuilder;
      const taskRegistry = c.get(Identifiers.TaskRegistry) as TaskRegistry;
      const workflowExecutionPool = c.get(
        Identifiers.WorkflowExecutionPool,
      ) as WorkflowExecutionPool;
      const stateManagerFactory = c.get(Identifiers.TriggerState);
      const checkpointStateManager = c.get(Identifiers.CheckpointState) as CheckpointState;
      const stateTransitorFactory = c.get(Identifiers.WorkflowStateTransitor);

      const factory: IdBasedServiceFactory<TriggerCoordinator> = {
        create: (executionId: string) => {
          const stateManager = (
            stateManagerFactory as unknown as IdBasedServiceFactory<TriggerState>
          ).create(executionId);
          const workflowStateTransitor = (
            stateTransitorFactory as unknown as IdBasedServiceFactory<WorkflowStateTransitor>
          ).create(executionId);
          const taskQueueManager = new TaskQueue(taskRegistry, workflowExecutionPool, eventManager);
          return new TriggerCoordinator({
            workflowExecutionRegistry: workflowExecutionRegistry,
            workflowRegistry,
            stateManager,
            checkpointStateManager,
            graphRegistry: workflowGraphRegistry,
            eventManager,
            executionBuilder: workflowExecutionBuilder,
            taskQueueManager,
            workflowLifecycleCoordinator: workflowStateTransitor,
            globalContext: c.get(Identifiers.GlobalContext) as GlobalContext,
          });
        },
      };
      return factory;
    })
    .inSingletonScope();

  // TriggeredSubworkflowHandler - Triggered Subworkflow Manager
  // As a singleton service, all triggered child workflows share the same Manager instance
  // Dependency on multiple services and managers
  container
    .bind(Identifiers.TriggeredSubworkflowHandler)
    .toDynamicValue((c: IContainer): TriggeredSubworkflowHandler => {
      const taskRegistry = c.get(Identifiers.TaskRegistry) as TaskRegistry;
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const workflowExecutionPool = c.get(
        Identifiers.WorkflowExecutionPool,
      ) as WorkflowExecutionPool;
      const taskQueueManager = new TaskQueue(taskRegistry, workflowExecutionPool, eventManager);
      const workflowExecutionRegistry = c.get(
        Identifiers.WorkflowExecutionRegistry,
      ) as WorkflowExecutionRegistry;
      // Create an adapter to convert null to undefined
      const workflowExecutionRegistryAdapter = {
        register: (entity: WorkflowExecutionEntity) => workflowExecutionRegistry.register(entity),
        get: (id: string) => workflowExecutionRegistry.get(id) ?? undefined,
      };
      return new TriggeredSubworkflowHandler(
        taskRegistry,
        workflowExecutionRegistryAdapter,
        c.get(Identifiers.WorkflowExecutionBuilder) as WorkflowExecutionBuilder,
        taskQueueManager,
        eventManager,
        workflowExecutionPool,
        c.get(Identifiers.AgentExecutionRegistry) as IAgentExecutionRegistry,
      );
    })
    .inSingletonScope();

  // WorkflowExecutionCoordinator - Workflow Execution Coordinator Factory
  // WorkflowExecutionCoordinator is responsible for coordinating the workflow execution process and requires WorkflowExecutionEntity as a parameter.
  // Create instances using the factory pattern, with one execution coordinator per workflow execution
  // Note: InterruptionState and NodeExecutionCoordinator need to be created based on executionId
  container
    .bind(Identifiers.WorkflowExecutionCoordinator)
    .toDynamicValue((c: IContainer) => {
      const interruptionManagerFactory = c.get(Identifiers.InterruptionState);
      const nodeExecutionCoordinatorFactory = c.get(Identifiers.NodeExecutionCoordinator);

      const factory: ExecutionEntityServiceFactory<WorkflowExecutionCoordinator> = {
        create: (executionEntity: WorkflowExecutionEntity) => {
          const executionId = executionEntity.id;
          const nodeId = executionEntity.getCurrentNodeId();
          const graph = executionEntity.getGraph();
          const navigator = new WorkflowNavigator(graph);

          // Create InterruptionState and set it on the entity
          const interruptionManager = (
            interruptionManagerFactory as unknown as InterruptionStateFactory<InterruptionState>
          ).create(executionId, nodeId);

          // Set the interruption state on the entity so getAbortSignal() returns the same signal
          executionEntity.setInterruptionState(interruptionManager);

          return new WorkflowExecutionCoordinator(
            executionEntity,
            interruptionManager,
            (
              nodeExecutionCoordinatorFactory as unknown as NodeExecutionCoordinatorFactory<NodeExecutionCoordinator>
            ).create(executionId, nodeId, executionEntity),
            navigator,
          );
        },
      };
      return factory;
    })
    .inSingletonScope();

  // ============================================================
  // Level 12: Execution Layer Coordinators (Medium to Low Priority)
  // ============================================================

  // AgentLoopExecutor - Agent Loop Executor Factory
  // Creates a new AgentLoopExecutor instance for each execution.
  container
    .bind(Identifiers.AgentLoopExecutor)
    .toDynamicValue((c: IContainer): NoArgServiceFactory<AgentLoopExecutor> => {
      const factory: NoArgServiceFactory<AgentLoopExecutor> = {
        create: () => {
          const llmExecutor = c.get(Identifiers.LLMExecutor) as LLMExecutor;
          const toolService = c.get(Identifiers.ToolRegistry) as ToolRegistry;
          const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
          return new AgentLoopExecutor({
            llmExecutor,
            toolService,
            globalContext,
            eventManager: globalContext?.eventRegistry,
          });
        },
      };
      return factory;
    })
    .inSingletonScope();

  // AgentLoopRegistry - The Agent Loop registry, a global singleton.
  // Depends on AgentLoopStorageAdapter for persistence
  container
    .bind(Identifiers.AgentLoopRegistry)
    .toDynamicValue((c: IContainer): AgentLoopRegistry => {
      const storageAdapter = c.get(
        Identifiers.AgentLoopStorageAdapter,
      ) as AgentLoopStorageAdapter | null;

      return new AgentLoopRegistry({
        storageAdapter: storageAdapter || undefined,
      });
    })
    .inSingletonScope();

  // AgentExecutionRegistry - Interface binding to AgentLoopRegistry singleton
  container
    .bind(Identifiers.AgentExecutionRegistry)
    .toDynamicValue((c: IContainer): IAgentExecutionRegistry => {
      return c.get(Identifiers.AgentLoopRegistry) as IAgentExecutionRegistry;
    })
    .inSingletonScope();

  // ExecutionHierarchyRegistry - Unified execution hierarchy registry (Phase 4)
  // Manages parent-child relationships across all execution types (Workflow, Agent)
  container
    .bind(Identifiers.ExecutionHierarchyRegistry)
    .to(ExecutionHierarchyRegistry)
    .inSingletonScope();

  // AgentLoopCoordinator - Agent Loop Lifecycle Coordinator Factory
  // Each time a new AgentLoopCoordinator instance is created
  container
    .bind(Identifiers.AgentLoopCoordinator)
    .toDynamicValue((c: IContainer): NoArgServiceFactory<AgentLoopCoordinator> => {
      const agentLoopExecutorFactory = c.get(Identifiers.AgentLoopExecutor);

      const factory: NoArgServiceFactory<AgentLoopCoordinator> = {
        create: () => {
          const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
          return new AgentLoopCoordinator(
            c.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry,
            (
              agentLoopExecutorFactory as unknown as NoArgServiceFactory<AgentLoopExecutor>
            ).create(),
            globalContext,
            c.get(Identifiers.EventRegistry) as EventRegistry | undefined, // EventRegistry for state transitor
          );
        },
      };
      return factory;
    })
    .inSingletonScope();

  // CheckpointCoordinator - Checkpoint Coordinator
  // Use static methods without instantiation
  // Provide a factory method to encapsulate dependent objects and static method calls
  container
    .bind(Identifiers.CheckpointCoordinator)
    .toDynamicValue((c: IContainer) => {
      const getFileCheckpointManager = (): FileCheckpointManager | undefined => {
        try {
          return c.isBound(Identifiers.FileCheckpointManager)
            ? (c.get(Identifiers.FileCheckpointManager) as FileCheckpointManager)
            : undefined;
        } catch {
          return undefined;
        }
      };

      return {
        dependencies: {
          workflowExecutionRegistry: c.get(
            Identifiers.WorkflowExecutionRegistry,
          ) as WorkflowExecutionRegistry,
          checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
          workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
          workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
          fileCheckpointManager: getFileCheckpointManager(),
        },
        createCheckpoint: (workflowExecutionId: string, metadata?: Record<string, unknown>) => {
          return CheckpointCoordinator.createCheckpoint(
            workflowExecutionId,
            {
              workflowExecutionRegistry: c.get(
                Identifiers.WorkflowExecutionRegistry,
              ) as WorkflowExecutionRegistry,
              checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
              workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
              workflowGraphRegistry: c.get(
                Identifiers.WorkflowGraphRegistry,
              ) as WorkflowGraphRegistry,
              fileCheckpointManager: getFileCheckpointManager(),
            },
            metadata,
          );
        },
        restoreFromCheckpoint: (checkpointId: string) => {
          return CheckpointCoordinator.restoreFromCheckpoint(checkpointId, {
            workflowExecutionRegistry: c.get(
              Identifiers.WorkflowExecutionRegistry,
            ) as WorkflowExecutionRegistry,
            checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
            workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
            workflowGraphRegistry: c.get(
              Identifiers.WorkflowGraphRegistry,
            ) as WorkflowGraphRegistry,
            fileCheckpointManager: getFileCheckpointManager(),
          });
        },
      };
    })
    .inSingletonScope();

  // ============================================================
  // Layer 12.5: Metrics Services (before WorkflowExecutionPool)
  // ============================================================

  // MetricsStorageAdapter - SQLite implementation for metrics persistence
  container
    .bind(Identifiers.MetricsStorageAdapter)
    .toDynamicValue(async (): Promise<MetricsStorageAdapter> => {
      const { SqliteMetricsStorage } = await import("@wf-agent/storage");
      const { join } = await import("path");

      const storage = new SqliteMetricsStorage({
        dbPath: join(process.cwd(), "data", "metrics.db"),
        enableWAL: true,
        vacuumInterval: 60 * 60 * 1000, // 1 hour
      });

      await storage.initialize();
      return storage;
    })
    .inSingletonScope();

  // ============================================================
  // Tool Permission Services (New Architecture)
  // ============================================================

  // RejectionMessageBuilder - Singleton service for building rejection messages
  container
    .bind(Identifiers.RejectionMessageBuilder)
    .toDynamicValue(() => new RejectionMessageBuilder())
    .inSingletonScope();

  // ToolPermissionManager - Factory pattern, one instance per execution
  // Will be initialized in WorkflowExecutionBuilder based on AvailableTools config
  container
    .bind(Identifiers.ToolPermissionManager)
    .toDynamicValue(() => {
      // This will be overridden by WorkflowExecutionBuilder during initialization
      // Return a placeholder that will be replaced
      return null;
    })
    .inTransientScope();

  // MetricsConfig - Load and merge metrics configuration with priority-based resolution
  // Priority: SDKOptions.metrics > Config file > Environment defaults > Hardcoded defaults
  container
    .bind(Identifiers.MetricsConfig)
    .toDynamicValue(async (c: IContainer): Promise<MetricsConfig> => {
      // Get SDK options from container
      const sdkOptions = c.get(Identifiers.SDKOptions) as SDKOptions | undefined;

      // Priority 1: SDKOptions.metrics (programmatic override)
      if (sdkOptions?.metrics) {
        logger.info("Using metrics config from SDKOptions");
        return mergeMetricsWithDefaults(sdkOptions.metrics);
      }

      // Priority 2: Config file
      const configPaths = ["./configs/metrics.toml", "./configs/metrics.json"];

      for (const path of configPaths) {
        try {
          const config = await loadMetricsConfigFromFile(path);
          logger.info("Loaded metrics config from file", { path });
          return config;
        } catch {
          // Try next path
        }
      }

      // Priority 3: Environment-based defaults
      const env = process.env["NODE_ENV"] || "development";
      logger.info("Using environment-based metrics defaults", { env });
      return getMetricsEnvironmentDefaults(env as "development" | "production");
    })
    .inSingletonScope();

  // MetricsRegistry - Unified metrics registry
  // Manages all metric collectors with centralized configuration
  container
    .bind(Identifiers.MetricsRegistry)
    .toDynamicValue((c: IContainer): MetricsRegistry => {
      const config = c.get(Identifiers.MetricsConfig) as MetricsConfig;

      // Only create registry if metrics are enabled
      if (config.enabled === false) {
        logger.info("Metrics collection is disabled");
        // Return a minimal registry with periodic reporting disabled
        return new MetricsRegistry({ enablePeriodicReporting: false });
      }

      logger.info("Initializing MetricsRegistry with configuration", {
        enabled: config.enabled,
        periodicReporting: config.enablePeriodicReporting,
        reportingInterval: config.reportingInterval,
      });

      return new MetricsRegistry(config);
    })
    .inSingletonScope();

  // TimeoutConfig - Load and merge timeout configuration with priority-based resolution
  // Priority: SDKOptions.timeout > Config file > Environment defaults > Hardcoded defaults
  container
    .bind(Identifiers.TimeoutConfig)
    .toDynamicValue(async (c: IContainer): Promise<Required<TimeoutConfig>> => {
      // Get SDK options from container
      const sdkOptions = c.get(Identifiers.SDKOptions) as SDKOptions | undefined;

      // Priority 1: SDKOptions.timeout (programmatic override)
      if (sdkOptions?.timeout) {
        logger.info("Using timeout config from SDKOptions");
        return mergeTimeoutWithDefaults(sdkOptions.timeout);
      }

      // Priority 2: Config file
      const configPaths = ["./configs/timeout.toml", "./configs/timeout.json"];

      for (const path of configPaths) {
        try {
          const config = await loadTimeoutConfigFromFile(path);
          logger.info("Loaded timeout config from file", { path });
          return config;
        } catch {
          // Try next path
        }
      }

      // Priority 3: Environment-based defaults
      const env = process.env["NODE_ENV"] || "development";
      logger.info("Using environment-based timeout defaults", { env });
      return getTimeoutEnvironmentDefaults(env as "development" | "production");
    })
    .inSingletonScope();

  // ============================================================
  // Layer 13: WorkflowExecutionPool (requires WorkflowExecutor, which must be bound after all dependencies)
  // ============================================================

  // WorkflowExecutionPool - workflow execution pool service for concurrent workflow execution
  // Note: It must be bound after all dependencies, as it requires WorkflowExecutor.
  // Per-instance pool to ensure isolation between SDK instances
  container
    .bind(Identifiers.WorkflowExecutionPool)
    .toDynamicValue((c: IContainer): WorkflowExecutionPool => {
      const config = {
        minExecutors: 1,
        maxExecutors: 10,
        idleTimeout: 30000,
        defaultTimeout: 30000,
      };

      // Create a new instance per container, not using singleton
      return new WorkflowExecutionPool(
        () => c.get(Identifiers.WorkflowExecutor) as WorkflowExecutor,
        config,
      );
    })
    .inSingletonScope();

  // ============================================================
  // File state snapshot wiring (deferred to CheckpointCoordinator)
  // The CheckpointCoordinator integrates file state via SqliteDelta
  // at the workflow level, not via VFS bridge.
  // ============================================================
}
