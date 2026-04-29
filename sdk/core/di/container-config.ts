/**
 * DI Container Configuration
 * Configure all service bindings in the DI container and define the dependencies between services.
 *
 * Design Principles:
 * - Configure service bindings in the order of dependencies to avoid circular dependencies.
 * - Use the singleton lifecycle for stateless global services (such as Registry, Manager).
 * - Use the factory pattern for thread-isolated services (such as GraphConversationSession, ConversationSession).
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
  CheckpointStorageCallback,
  WorkflowStorageCallback,
  TaskStorageCallback,
} from "@wf-agent/storage";
import * as Identifiers from "./service-identifiers.js";

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
import { TaskRegistry } from "../../workflow/stores/task/task-registry.js";
import { TaskQueue } from "../../workflow/stores/task/task-queue.js";
import { WorkflowRegistry } from "../../workflow/stores/workflow-registry.js";
import { WorkflowExecutionPool } from "../../workflow/execution/workflow-execution-pool.js";

// Execution Layer Services - Core Layer Universal Executor
import { LLMExecutor, ToolCallExecutor } from "../executors/index.js";
import { ToolApprovalCoordinator } from "../coordinators/tool-approval-coordinator.js";
import { SkillRegistry } from "../registry/skill-registry.js";
import { SkillLoader } from "../utils/skill-loader.js";
import { safeEmit } from "../../workflow/execution/utils/index.js";
import { createCheckpoint } from "../../workflow/checkpoint/utils/checkpoint-utils.js";
import {
  buildMessageAddedEvent,
  buildToolCallStartedEvent,
  buildToolCallCompletedEvent,
  buildToolCallFailedEvent,
} from "../../workflow/execution/utils/event/index.js";
import { WorkflowStateTransitor } from "../../workflow/execution/coordinators/workflow-state-transitor.js";
import { CheckpointState } from "../../workflow/checkpoint/checkpoint-state-manager.js";
import { ToolContextStore } from "../../workflow/stores/tool-context-store.js";
import { WorkflowConversationSession } from "../../workflow/message/workflow-conversation-session.js";
import { ToolVisibilityStore } from "../../workflow/stores/tool-visibility-store.js";
import { ToolVisibilityCoordinator } from "../../workflow/execution/coordinators/tool-visibility-coordinator.js";
import { WorkflowExecutionBuilder } from "../../workflow/execution/factories/workflow-execution-builder.js";
import { WorkflowExecutor } from "../../workflow/execution/executors/workflow-executor.js";

// Execution Layer - Coordinators
import { WorkflowExecutionCoordinator } from "../../workflow/execution/coordinators/workflow-execution-coordinator.js";
import { VariableCoordinator } from "../../workflow/execution/coordinators/variable-coordinator.js";
import { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import { NodeExecutionCoordinator } from "../../workflow/execution/coordinators/node-execution-coordinator.js";
import { TriggeredSubworkflowHandler } from "../../workflow/execution/handlers/triggered-subworkflow-handler.js";
import { LLMExecutionCoordinator } from "../../workflow/execution/coordinators/llm-execution-coordinator.js";
import { CheckpointCoordinator } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import { WorkflowNavigator } from "../../workflow/builder/workflow-navigator.js";
import { WorkflowLifecycleCoordinator } from "../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js";

// Execution Layer - Managers
import { ConversationSession } from "../messaging/conversation-session.js";
import { VariableState } from "../../workflow/state-managers/variable-state.js";
import { TriggerState } from "../../workflow/state-managers/trigger-state.js";
import { InterruptionState } from "../types/interruption-state.js";
import { AgentLoopExecutor } from "../../agent/execution/executors/agent-loop-executor.js";
import { AgentLoopRegistry } from "../../agent/loop/agent-loop-registry.js";
import { AgentLoopCoordinator } from "../../agent/execution/coordinators/agent-loop-coordinator.js";
import { WorkflowExecutionEntity } from "../../workflow/entities/workflow-execution-entity.js";

/** Global container instance */
let container: Container | null = null;
/** Storage callbacks provided by the application layer */
let storageCallback: CheckpointStorageCallback | null = null;
let workflowStorageCallback: WorkflowStorageCallback | null = null;
let taskStorageCallback: TaskStorageCallback | null = null;

/**
 * Set checkpoint storage callback
 * @param callback Implementation of the checkpoint storage callback interface
 */
export function setStorageCallback(callback: CheckpointStorageCallback): void {
  storageCallback = callback;
}

/**
 * Get the checkpoint storage callback
 * @returns Implementation of the checkpoint storage callback interface
 * @throws Error If the storage callback is not initialized
 */
export function getStorageCallback(): CheckpointStorageCallback {
  if (!storageCallback) {
    throw new Error(
      "Storage callback not initialized. " + "Please call setStorageCallback() before using SDK.",
    );
  }
  return storageCallback;
}

/**
 * Set workflow storage callback
 * @param callback Implementation of the workflow storage callback interface
 */
export function setWorkflowStorageCallback(callback: WorkflowStorageCallback): void {
  workflowStorageCallback = callback;
}

/**
 * Get the workflow storage callback
 * @returns Implementation of the workflow storage callback interface or null
 */
export function getWorkflowStorageCallback(): WorkflowStorageCallback | null {
  return workflowStorageCallback;
}

/**
 * Set thread storage callback
 * @param callback Implementation of the thread storage callback interface
 */
export function setTaskStorageCallback(callback: TaskStorageCallback): void {
  taskStorageCallback = callback;
}

/**
 * Get the thread storage callback
 * @returns Implementation of the thread storage callback interface or null
 */
export function getTaskStorageCallback(): TaskStorageCallback | null {
  return taskStorageCallback;
}

/**
 * Initialize the DI container
 * Configure all service bindings in the order of dependencies
 *
 * @param storageCallback Implementation of the storage callback interface (optional; if not provided, setStorageCallback must be called before initialization)
 * @returns The configured container instance
 */
export function initializeContainer(storageCallback?: CheckpointStorageCallback): Container {
  if (container) {
    return container;
  }

  container = new Container();

  // If a storageCallback is provided, set it.
  if (storageCallback) {
    setStorageCallback(storageCallback);
  }

  // ============================================================
  // First Layer: A storage layer service with no dependencies
  // ============================================================

  container.bind(Identifiers.WorkflowGraphRegistry).to(WorkflowGraphRegistry).inSingletonScope();

  container.bind(Identifiers.WorkflowExecutionRegistry).to(WorkflowExecutionRegistry).inSingletonScope();

  // LLMWrapper - An LLM wrapper that relies on EventRegistry for event publishing.
  (container as any)
    .bind(Identifiers.LLMWrapper)
    .toDynamicValue(
      (c: IContainer) => new LLMWrapper(c.get(Identifiers.EventRegistry) as EventRegistry),
    )
    .inSingletonScope();

  // ============================================================
  // Level 2: Business layer services with no dependencies
  // ============================================================

  container.bind(Identifiers.EventRegistry).to(EventRegistry).inSingletonScope();

  container.bind(Identifiers.ToolRegistry).to(ToolRegistry).inSingletonScope();

  container.bind(Identifiers.ScriptRegistry).to(ScriptRegistry).inSingletonScope();

  container.bind(Identifiers.NodeTemplateRegistry).to(NodeTemplateRegistry).inSingletonScope();

  container
    .bind(Identifiers.TriggerTemplateRegistry)
    .to(TriggerTemplateRegistry)
    .inSingletonScope();

  container
    .bind(Identifiers.TaskRegistry)
    .toDynamicValue(() => TaskRegistry.getInstance())
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

  // WorkflowRegistry - A workflow registry that relies on WorkflowExecutionRegistry for reference checking.
  container
    .bind(Identifiers.WorkflowRegistry)
    .toDynamicValue((c: IContainer): WorkflowRegistry => {
      const workflowExecutionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
      return new WorkflowRegistry({ maxRecursionDepth: 10 }, workflowExecutionRegistry);
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

      return new ToolCallExecutor(
        c.get(Identifiers.ToolRegistry) as ToolRegistry,
        c.get(Identifiers.EventRegistry) as EventRegistry,
        {
          workflowExecutionRegistry: c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry,
          checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
          workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
          workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
        },
        c.get(Identifiers.ToolVisibilityStore) as ToolVisibilityStore,
        eventBuilder,
        createCheckpoint,
        safeEmit,
      );
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

  // WorkflowConversationSession - Workflow Conversation Session Factory
  // WorkflowConversationSession is thread-isolated and requires a separate instance for each thread.
  // Create instances using the factory pattern to ensure data isolation between threads
  container
    .bind(Identifiers.GraphConversationSession)
    .toDynamicValue(() => {
      return {
        create: (executionId: string) => new WorkflowConversationSession({ executionId }),
      };
    })
    .inSingletonScope();

  // WorkflowStateTransitor - Workflow State Transitor Factory
  // Each workflow execution requires a separate instance of the state transitor
  container
    .bind(Identifiers.WorkflowStateTransitor)
    .toDynamicValue((c: IContainer): { create: (executionId: string) => WorkflowStateTransitor } => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const workflowConversationSessionFactory = c.get(Identifiers.GraphConversationSession) as {
        create: (executionId: string) => WorkflowConversationSession;
      };
      const workflowExecutionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
      const taskRegistry = c.get(Identifiers.TaskRegistry) as TaskRegistry;
      return {
        create: (executionId: string) => {
          const workflowConversationSession = workflowConversationSessionFactory.create(executionId);
          return new WorkflowStateTransitor(
            eventManager,
            workflowConversationSession,
            workflowExecutionRegistry,
          );
        },
      };
    })
    .inSingletonScope();

  // ToolContextStore - A tool context store with no dependencies.
  container.bind(Identifiers.ToolContextStore).to(ToolContextStore).inSingletonScope();

  // ToolVisibilityStore - Tool Visibility Store (stateful), no dependencies
  container.bind(Identifiers.ToolVisibilityStore).to(ToolVisibilityStore).inSingletonScope();

  // ============================================================
  // Layer six: Depends on the execution layer services of layer five
  // ============================================================

  // CheckpointState - Checkpoint State Manager
  // Requires CheckpointStorageCallback implementation from the application layer.
  // Storage callbacks can be provided in two ways:
  // 1. 在调用 initializeContainer() 时传入 storageCallback 参数
  // 2. 在初始化容器前调用 setStorageCallback() 函数
  container
    .bind(Identifiers.CheckpointState)
    .toDynamicValue((c: IContainer): CheckpointState => {
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const callback = getStorageCallback();

      if (!callback) {
        throw new Error(
          "CheckpointState requires a CheckpointStorageCallback implementation. " +
            "Please provide it either via initializeContainer(storageCallback) parameter " +
            "or by calling setStorageCallback() before initialization.",
        );
      }

      return new CheckpointState(callback, eventManager);
    })
    .inSingletonScope();

  // ============================================================
  // Layer 7: WorkflowExecutor (depends on WorkflowGraphRegistry and WorkflowExecutionCoordinator factories)
  // ============================================================

  // WorkflowExecutor - A workflow executor that relies on the WorkflowGraphRegistry and WorkflowExecutionCoordinator factories.
  container
    .bind(Identifiers.WorkflowExecutor)
    .toDynamicValue((c: IContainer): WorkflowExecutor => {
      return new WorkflowExecutor({
        workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
        workflowExecutionCoordinatorFactory: c.get(Identifiers.WorkflowExecutionCoordinator) as {
          create: (executionEntity: WorkflowExecutionEntity) => WorkflowExecutionCoordinator;
        },
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
    .toDynamicValue(
      (c: IContainer): { create: (executionId: string) => WorkflowLifecycleCoordinator } => {
        const workflowExecutionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
        const workflowExecutionBuilder = c.get(Identifiers.WorkflowExecutionBuilder) as WorkflowExecutionBuilder;
        const workflowExecutor = c.get(Identifiers.WorkflowExecutor) as WorkflowExecutor;
        const stateTransitorFactory = c.get(Identifiers.WorkflowStateTransitor) as {
          create: (executionId: string) => WorkflowStateTransitor;
        };

        return {
          create: (executionId: string) => {
            const stateTransitor = stateTransitorFactory.create(executionId);
            return new WorkflowLifecycleCoordinator(
              workflowExecutionRegistry,
              stateTransitor,
              workflowExecutionBuilder,
              workflowExecutor,
            );
          },
        };
      },
    )
    .inSingletonScope();

  // ============================================================
  // Layer 9: WorkflowExecutionBuilder (no dependencies)
  // ============================================================

  // WorkflowExecutionBuilder - A workflow execution builder with no dependencies
  container.bind(Identifiers.WorkflowExecutionBuilder).to(WorkflowExecutionBuilder).inSingletonScope();

  // ============================================================
  // Level 10: Basic Managers at the Execution Layer (without dependencies or factory patterns)
  // ============================================================

  // VariableState - Variable State Manager Factory
  // VariableState requires executionId, use factory pattern to create instance
  container
    .bind(Identifiers.VariableState)
    .toDynamicValue(() => {
      return {
        create: (executionId: string) => new VariableState(executionId),
      };
    })
    .inSingletonScope();

  // TriggerState - Trigger State Manager Factory
  // TriggerState requires executionId, use factory pattern to create instance
  container
    .bind(Identifiers.TriggerState)
    .toDynamicValue(() => {
      return {
        create: (executionId: string) => new TriggerState(executionId),
      };
    })
    .inSingletonScope();

  // InterruptionState - Interruption Manager Factory
  // InterruptionState requires executionId and nodeId, and uses the factory pattern to create instances.
  container
    .bind(Identifiers.InterruptionState)
    .toDynamicValue(() => {
      return {
        create: (executionId: string, nodeId: string) => new InterruptionState(executionId, nodeId),
      };
    })
    .inSingletonScope();

  // ============================================================
  // Layer Eleven: Execution Layer Coordinators (High Priority)
  // ============================================================

  // VariableCoordinator - A variable coordinator that relies on VariableState and EventRegistry
  container
    .bind(Identifiers.VariableCoordinator)
    .toDynamicValue((c: IContainer): VariableCoordinator => {
      const stateManager = c.get(Identifiers.VariableState) as VariableState;
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      return new VariableCoordinator(stateManager, eventManager);
    })
    .inSingletonScope();

  // LLMExecutionCoordinator - 依赖 LLMExecutor、ToolRegistry、EventRegistry、ToolCallExecutor
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
  // ConversationSession is stateful and requires a separate instance for each thread to achieve thread isolation
  // Creating instances using the factory pattern ensures that the session data for each thread is independent of each other
  container
    .bind(Identifiers.ConversationSession)
    .toDynamicValue(
      (
        c: IContainer,
      ): { create: (executionId?: string, workflowId?: string) => ConversationSession } => {
        const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;

        return {
          create: (executionId?: string, workflowId?: string) => {
            return new ConversationSession({
              eventManager,
              executionId,
              workflowId,
            });
          },
        };
      },
    )
    .inSingletonScope();

  // NodeExecutionCoordinator - dependency on multiple services and coordinators
  // NodeExecutionCoordinator - node execution coordinator factory
  // Each workflow execution requires a separate node execution coordinator instance
  container
    .bind(Identifiers.NodeExecutionCoordinator)
    .toDynamicValue(
      (
        c: IContainer,
      ): {
        create: (
          executionId: string,
          nodeId: string,
          executionEntity: WorkflowExecutionEntity,
        ) => NodeExecutionCoordinator;
      } => {
        const conversationManagerFactory = c.get(Identifiers.ConversationSession) as {
          create: (executionId?: string, workflowId?: string) => ConversationSession;
        };
        const interruptionManagerFactory = c.get(Identifiers.InterruptionState) as {
          create: (executionId: string, nodeId: string) => InterruptionState;
        };
        const agentLoopExecutorFactory = c.get(Identifiers.AgentLoopExecutor) as {
          create: () => AgentLoopExecutor;
        };

        return {
          create: (executionId: string, nodeId: string, executionEntity: WorkflowExecutionEntity) => {
            const graph = executionEntity.getGraph();
            const navigator = new WorkflowNavigator(graph);
            const config = {
              eventManager: c.get(Identifiers.EventRegistry) as EventRegistry,
              llmCoordinator: c.get(Identifiers.LLMExecutionCoordinator) as LLMExecutionCoordinator,
              conversationManager: conversationManagerFactory.create(executionId),
              interruptionManager: interruptionManagerFactory.create(executionId, nodeId),
              navigator,
              toolService: c.get(Identifiers.ToolRegistry) as ToolRegistry,
              toolContextStore: c.get(Identifiers.ToolContextStore) as ToolContextStore,
              checkpointDependencies: {
                workflowExecutionRegistry: c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry,
                checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
                workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
                workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
              },
              agentLoopExecutorFactory: agentLoopExecutorFactory.create(),
            };
            return new NodeExecutionCoordinator(config);
          },
        };
      },
    )
    .inSingletonScope();

  // TriggerCoordinator - Dependencies on multiple services and coordinators
  // TriggerCoordinator - Trigger Coordinator Factory
  // Each workflow execution requires a separate instance of the trigger coordinator
  container
    .bind(Identifiers.TriggerCoordinator)
    .toDynamicValue((c: IContainer): { create: (executionId: string) => TriggerCoordinator } => {
      const workflowExecutionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
      const workflowRegistry = c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry;
      const workflowGraphRegistry = c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
      const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
      const workflowExecutionBuilder = c.get(Identifiers.WorkflowExecutionBuilder) as WorkflowExecutionBuilder;
      const taskRegistry = c.get(Identifiers.TaskRegistry) as TaskRegistry;
      const workflowExecutionPool = c.get(Identifiers.WorkflowExecutionPool) as WorkflowExecutionPool;
      const stateManagerFactory = c.get(Identifiers.TriggerState) as {
        create: (executionId: string) => TriggerState;
      };
      const checkpointStateManager = c.get(Identifiers.CheckpointState) as CheckpointState;
      const stateTransitorFactory = c.get(Identifiers.WorkflowStateTransitor) as {
        create: (executionId: string) => WorkflowStateTransitor;
      };

      return {
        create: (executionId: string) => {
          const stateManager = stateManagerFactory.create(executionId);
          const workflowStateTransitor = stateTransitorFactory.create(executionId);
          const taskQueueManager = new TaskQueue(taskRegistry, workflowExecutionPool, eventManager);
          return new TriggerCoordinator({
            workflowExecutionRegistry: workflowExecutionRegistry,
            workflowRegistry,
            stateManager,
            checkpointStateManager,
            workflowGraphRegistry: workflowGraphRegistry,
            eventManager,
            executionBuilder: workflowExecutionBuilder,
            taskQueueManager,
            threadLifecycleCoordinator: workflowStateTransitor,
          });
        },
      };
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
      const workflowExecutionPool = c.get(Identifiers.WorkflowExecutionPool) as WorkflowExecutionPool;
      const taskQueueManager = new TaskQueue(taskRegistry, workflowExecutionPool, eventManager);
      const workflowExecutionRegistry = c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
      // Create an adapter to convert null to undefined
      const workflowExecutionRegistryAdapter = {
        register: (entity: WorkflowExecutionEntity) => workflowExecutionRegistry.register(entity),
        get: (id: string) => workflowExecutionRegistry.get(id) ?? undefined,
      };
      return new TriggeredSubworkflowHandler(
        workflowExecutionRegistryAdapter,
        c.get(Identifiers.WorkflowExecutionBuilder) as WorkflowExecutionBuilder,
        taskQueueManager,
        eventManager,
        workflowExecutionPool,
      );
    })
    .inSingletonScope();

  // WorkflowExecutionCoordinator - Workflow Execution Coordinator Factory
  // WorkflowExecutionCoordinator is responsible for coordinating the workflow execution process and requires WorkflowExecutionEntity as a parameter.
  // Create instances using the factory pattern, with one execution coordinator per workflow execution
  // Note: VariableCoordinator、TriggerCoordinator、InterruptionState、ToolVisibilityCoordinator、NodeExecutionCoordinator all need to be created based on executionId
  container
    .bind(Identifiers.WorkflowExecutionCoordinator)
    .toDynamicValue(
      (c: IContainer): { create: (executionEntity: WorkflowExecutionEntity) => WorkflowExecutionCoordinator } => {
        const variableCoordinator = c.get(Identifiers.VariableCoordinator) as VariableCoordinator;
        const workflowGraphRegistry = c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
        const toolService = c.get(Identifiers.ToolRegistry) as ToolRegistry;
        const toolVisibilityStore = c.get(Identifiers.ToolVisibilityStore) as ToolVisibilityStore;
        const toolVisibilityCoordinator = new ToolVisibilityCoordinator(
          toolService,
          toolVisibilityStore,
        );
        const triggerCoordinatorFactory = c.get(Identifiers.TriggerCoordinator) as {
          create: (executionId: string) => TriggerCoordinator;
        };
        const interruptionManagerFactory = c.get(Identifiers.InterruptionState) as {
          create: (executionId: string, nodeId: string) => InterruptionState;
        };
        const nodeExecutionCoordinatorFactory = c.get(Identifiers.NodeExecutionCoordinator) as {
          create: (
            executionId: string,
            nodeId: string,
            executionEntity: WorkflowExecutionEntity,
          ) => NodeExecutionCoordinator;
        };

        return {
          create: (executionEntity: WorkflowExecutionEntity) => {
            const executionId = executionEntity.id;
            const nodeId = executionEntity.getCurrentNodeId();
            const graph = executionEntity.getGraph();
            const navigator = new WorkflowNavigator(graph);
            return new WorkflowExecutionCoordinator(
              executionEntity,
              variableCoordinator,
              triggerCoordinatorFactory.create(executionId),
              interruptionManagerFactory.create(executionId, nodeId),
              toolVisibilityCoordinator,
              nodeExecutionCoordinatorFactory.create(executionId, nodeId, executionEntity),
              navigator,
            );
          },
        };
      },
    )
    .inSingletonScope();

  // ============================================================
  // Level 12: Execution Layer Coordinators (Medium to Low Priority)
  // ============================================================

  // AgentLoopExecutor - Agent Loop Executor Factory
  // Creates a new AgentLoopExecutor instance for each execution.
  container
    .bind(Identifiers.AgentLoopExecutor)
    .toDynamicValue((c: IContainer): { create: () => AgentLoopExecutor } => {
      return {
        create: () => {
          const llmExecutor = c.get(Identifiers.LLMExecutor) as LLMExecutor;
          const toolService = c.get(Identifiers.ToolRegistry) as ToolRegistry;
          return new AgentLoopExecutor(llmExecutor, toolService);
        },
      };
    })
    .inSingletonScope();

  // AgentLoopRegistry - The Agent Loop registry, a global singleton.
  container.bind(Identifiers.AgentLoopRegistry).to(AgentLoopRegistry).inSingletonScope();

  // AgentLoopCoordinator - Agent Loop Lifecycle Coordinator Factory
  // Each time a new AgentLoopCoordinator instance is created
  container
    .bind(Identifiers.AgentLoopCoordinator)
    .toDynamicValue((c: IContainer): { create: () => AgentLoopCoordinator } => {
      return {
        create: () => {
          return new AgentLoopCoordinator(
            c.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry,
            (c.get(Identifiers.AgentLoopExecutor) as { create: () => AgentLoopExecutor }).create(),
          );
        },
      };
    })
    .inSingletonScope();

  // CheckpointCoordinator - Checkpoint Coordinator
  // Use static methods without instantiation
  // Provide a factory method to encapsulate dependent objects and static method calls
  container
    .bind(Identifiers.CheckpointCoordinator)
    .toDynamicValue((c: IContainer) => {
      return {
        dependencies: {
          workflowExecutionRegistry: c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry,
          checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
          workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
          workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
        },
        createCheckpoint: (workflowExecutionId: string, metadata?: Record<string, unknown>) => {
          return CheckpointCoordinator.createCheckpoint(
            workflowExecutionId,
            {
              workflowExecutionRegistry: c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry,
              checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
              workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
              workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
            },
            metadata,
          );
        },
        restoreFromCheckpoint: (checkpointId: string) => {
          return CheckpointCoordinator.restoreFromCheckpoint(checkpointId, {
            workflowExecutionRegistry: c.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry,
            checkpointStateManager: c.get(Identifiers.CheckpointState) as CheckpointState,
            workflowRegistry: c.get(Identifiers.WorkflowRegistry) as WorkflowRegistry,
            workflowGraphRegistry: c.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry,
          });
        },
      };
    })
    .inSingletonScope();

  // ============================================================
  // Layer 13: WorkflowExecutionPool (requires WorkflowExecutor, which must be bound after all dependencies)
  // ============================================================

  // WorkflowExecutionPool - workflow execution pool service for concurrent workflow execution
  // Note: It must be bound after all dependencies, as it requires WorkflowExecutor.
  container
    .bind(Identifiers.WorkflowExecutionPool)
    .toDynamicValue((c: IContainer): WorkflowExecutionPool => {
      const config = {
        minExecutors: 1,
        maxExecutors: 10,
        idleTimeout: 30000,
        defaultTimeout: 30000,
      };

      return WorkflowExecutionPool.getInstance(
        () => c.get(Identifiers.WorkflowExecutor) as WorkflowExecutor,
        config,
      );
    })
    .inSingletonScope();

  return container;
}

/**
 * Get a DI container instance
 *
 * @returns The container instance
 * @throws Error If the container is not initialized
 */
export function getContainer(): Container {
  if (!container) {
    throw new Error("Container not initialized. Call initializeContainer() first.");
  }
  return container;
}

/**
 * Reset DI container
 * Clear all caches and service instances, mainly used for test environments
 * After calling, you need to call initializeContainer() again to initialize the container
 */
export function resetContainer(): void {
  if (container) {
    container.clearAllCaches();
    container = null;
  }
  storageCallback = null;
  workflowStorageCallback = null;
  taskStorageCallback = null;
}

/**
 * Check if the container has been initialized.
 *
 * @returns Whether it has been initialized
 */
export function isContainerInitialized(): boolean {
  return container !== null;
}
