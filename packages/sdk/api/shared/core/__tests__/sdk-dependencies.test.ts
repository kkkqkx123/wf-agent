import { describe, it, expect, beforeEach, vi } from "vitest";
import { APIDependencyManager } from "../sdk-dependencies.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import type { Container } from "@wf-agent/common-utils";
import * as Identifiers from "../../../../di/service-identifiers.js";

describe("sdk-dependencies.ts", () => {
  let mockGlobalContext: GlobalContext;
  let mockContainer: Container;
  let dependencyManager: APIDependencyManager;
  let mockWorkflowLifecycleCoordinatorCreate: ReturnType<typeof vi.fn>;
  let mockAgentLoopCoordinatorCreate: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockWorkflowLifecycleCoordinatorCreate = vi.fn(() => ({ test: "coordinator" }));
    mockAgentLoopCoordinatorCreate = vi.fn(() => ({ test: "agentLoopCoordinator" }));

    mockContainer = {
      get: vi.fn((id: unknown) => {
        if (id === Identifiers.WorkflowExecutionRegistry) {
          return { test: "workflowExecutionRegistry" };
        }
        if (id === Identifiers.CheckpointState) {
          return { test: "checkpointState" };
        }
        if (id === Identifiers.WorkflowGraphRegistry) {
          return { test: "workflowGraphRegistry" };
        }
        if (id === Identifiers.WorkflowLifecycleCoordinator) {
          return {
            create: mockWorkflowLifecycleCoordinatorCreate,
          };
        }
        if (id === Identifiers.LLMWrapper) {
          return { test: "llmWrapper" };
        }
        if (id === Identifiers.SkillRegistry) {
          return { test: "skillRegistry" };
        }
        if (id === Identifiers.AgentLoopRegistry) {
          return { test: "agentLoopRegistry" };
        }
        if (id === Identifiers.AgentLoopCoordinator) {
          return {
            create: mockAgentLoopCoordinatorCreate,
          };
        }
        if (id === Identifiers.TaskRegistry) {
          return { test: "taskRegistry" };
        }
        if (id === Identifiers.CheckpointStorageAdapter) {
          return { test: "checkpointStorageAdapter" };
        }
        if (id === Identifiers.WorkflowStorageAdapter) {
          return { test: "workflowStorageAdapter" };
        }
        if (id === Identifiers.WorkflowExecutionStorageAdapter) {
          return { test: "workflowExecutionStorageAdapter" };
        }
        if (id === Identifiers.TaskStorageAdapter) {
          return { test: "taskStorageAdapter" };
        }
        if (id === Identifiers.FileCheckpointManager) {
          return { test: "fileCheckpointManager" };
        }
        if (id === Identifiers.MetricsRegistry) {
          return { test: "metricsRegistry" };
        }
        return null;
      }),
    } as unknown as Container;

    mockGlobalContext = {
      container: mockContainer,
      workflowRegistry: { test: "workflowRegistry" },
      eventRegistry: { test: "eventRegistry" },
      toolRegistry: { test: "toolRegistry" },
      llmExecutor: { test: "llmExecutor" },
      scriptRegistry: { test: "scriptRegistry" },
      scriptExecutor: { test: "scriptExecutor" },
      nodeTemplateRegistry: { test: "nodeTemplateRegistry" },
      triggerTemplateRegistry: { test: "triggerTemplateRegistry" },
      hookTemplateRegistry: { test: "hookTemplateRegistry" },
    } as unknown as GlobalContext;

    dependencyManager = new APIDependencyManager(mockGlobalContext);
  });

  describe("constructor", () => {
    it("should create APIDependencyManager with globalContext", () => {
      expect(dependencyManager).toBeDefined();
    });

    it("should store globalContext reference", () => {
      expect(
        (dependencyManager as unknown as { globalContext: GlobalContext }).globalContext,
      ).toBe(mockGlobalContext);
    });
  });

  describe("getWorkflowRegistry", () => {
    it("should return workflow registry from globalContext", () => {
      const registry = dependencyManager.getWorkflowRegistry();
      expect(registry).toEqual({ test: "workflowRegistry" });
    });
  });

  describe("getWorkflowExecutionRegistry", () => {
    it("should return workflow execution registry from container", () => {
      const registry = dependencyManager.getWorkflowExecutionRegistry();
      expect(registry).toEqual({ test: "workflowExecutionRegistry" });
    });

    it("should call container.get with correct identifier", () => {
      dependencyManager.getWorkflowExecutionRegistry();
      expect(mockContainer.get).toHaveBeenCalledWith(
        Identifiers.WorkflowExecutionRegistry,
      );
    });
  });

  describe("getEventManager", () => {
    it("should return event registry from globalContext", () => {
      const registry = dependencyManager.getEventManager();
      expect(registry).toEqual({ test: "eventRegistry" });
    });
  });

  describe("getCheckpointStateManager", () => {
    it("should return checkpoint state from container", () => {
      const state = dependencyManager.getCheckpointStateManager();
      expect(state).toEqual({ test: "checkpointState" });
    });

    it("should call container.get with correct identifier", () => {
      dependencyManager.getCheckpointStateManager();
      expect(mockContainer.get).toHaveBeenCalledWith(Identifiers.CheckpointState);
    });
  });

  describe("getToolService", () => {
    it("should return tool registry from globalContext", () => {
      const service = dependencyManager.getToolService();
      expect(service).toEqual({ test: "toolRegistry" });
    });
  });

  describe("getLlmExecutor", () => {
    it("should return LLM executor from globalContext", () => {
      const executor = dependencyManager.getLlmExecutor();
      expect(executor).toEqual({ test: "llmExecutor" });
    });
  });

  describe("getScriptService", () => {
    it("should return script registry from globalContext", () => {
      const service = dependencyManager.getScriptService();
      expect(service).toEqual({ test: "scriptRegistry" });
    });
  });

  describe("getScriptExecutor", () => {
    it("should return script executor from globalContext", () => {
      const executor = dependencyManager.getScriptExecutor();
      expect(executor).toEqual({ test: "scriptExecutor" });
    });
  });

  describe("getNodeTemplateRegistry", () => {
    it("should return node template registry from globalContext", () => {
      const registry = dependencyManager.getNodeTemplateRegistry();
      expect(registry).toEqual({ test: "nodeTemplateRegistry" });
    });
  });

  describe("getTriggerTemplateRegistry", () => {
    it("should return trigger template registry from globalContext", () => {
      const registry = dependencyManager.getTriggerTemplateRegistry();
      expect(registry).toEqual({ test: "triggerTemplateRegistry" });
    });
  });

  describe("getHookTemplateRegistry", () => {
    it("should return hook template registry from globalContext", () => {
      const registry = dependencyManager.getHookTemplateRegistry();
      expect(registry).toEqual({ test: "hookTemplateRegistry" });
    });
  });

  describe("getWorkflowGraphRegistry", () => {
    it("should return workflow graph registry from container", () => {
      const registry = dependencyManager.getWorkflowGraphRegistry();
      expect(registry).toEqual({ test: "workflowGraphRegistry" });
    });
  });

  describe("getWorkflowLifecycleCoordinator", () => {
    it("should create workflow lifecycle coordinator from factory", () => {
      const coordinator = dependencyManager.getWorkflowLifecycleCoordinator();
      expect(coordinator).toEqual({ test: "coordinator" });
    });

    it("should call create with empty string", () => {
      dependencyManager.getWorkflowLifecycleCoordinator();
      expect(mockWorkflowLifecycleCoordinatorCreate).toHaveBeenCalledWith("");
    });
  });

  describe("getLLMWrapper", () => {
    it("should return LLM wrapper from container", () => {
      const wrapper = dependencyManager.getLLMWrapper();
      expect(wrapper).toEqual({ test: "llmWrapper" });
    });
  });

  describe("getSkillRegistry", () => {
    it("should return skill registry from container", () => {
      const registry = dependencyManager.getSkillRegistry();
      expect(registry).toEqual({ test: "skillRegistry" });
    });
  });

  describe("getAgentLoopRegistry", () => {
    it("should return agent loop registry from container", () => {
      const registry = dependencyManager.getAgentLoopRegistry();
      expect(registry).toEqual({ test: "agentLoopRegistry" });
    });
  });

  describe("getAgentLoopCoordinator", () => {
    it("should create agent loop coordinator from factory", () => {
      const coordinator = dependencyManager.getAgentLoopCoordinator();
      expect(coordinator).toEqual({ test: "agentLoopCoordinator" });
    });

    it("should call create with no arguments", () => {
      dependencyManager.getAgentLoopCoordinator();
      expect(mockAgentLoopCoordinatorCreate).toHaveBeenCalled();
    });
  });

  describe("getTaskRegistry", () => {
    it("should return task registry from container", () => {
      const registry = dependencyManager.getTaskRegistry();
      expect(registry).toEqual({ test: "taskRegistry" });
    });
  });

  describe("getCheckpointStorageAdapter", () => {
    it("should return checkpoint storage adapter from container when available", () => {
      const adapter = dependencyManager.getCheckpointStorageAdapter();
      expect(adapter).toEqual({ test: "checkpointStorageAdapter" });
    });

    it("should return null when adapter not in container", () => {
      const mockContainerNoAdapter = {
        get: vi.fn((id: unknown) => {
          if (id === Identifiers.CheckpointStorageAdapter) {
            throw new Error("not found");
          }
          return null;
        }),
      } as unknown as Container;

      const mockContextNoAdapter = {
        container: mockContainerNoAdapter,
      } as unknown as GlobalContext;

      const manager = new APIDependencyManager(mockContextNoAdapter);
      const adapter = manager.getCheckpointStorageAdapter();
      expect(adapter).toBeNull();
    });
  });

  describe("getWorkflowStorageAdapter", () => {
    it("should return workflow storage adapter from container when available", () => {
      const adapter = dependencyManager.getWorkflowStorageAdapter();
      expect(adapter).toEqual({ test: "workflowStorageAdapter" });
    });

    it("should return null when adapter not in container", () => {
      const mockContainerNoAdapter = {
        get: vi.fn((id: unknown) => {
          if (id === Identifiers.WorkflowStorageAdapter) {
            throw new Error("not found");
          }
          return null;
        }),
      } as unknown as Container;

      const mockContextNoAdapter = {
        container: mockContainerNoAdapter,
      } as unknown as GlobalContext;

      const manager = new APIDependencyManager(mockContextNoAdapter);
      const adapter = manager.getWorkflowStorageAdapter();
      expect(adapter).toBeNull();
    });
  });

  describe("getWorkflowExecutionStorageAdapter", () => {
    it("should return workflow execution storage adapter when available", () => {
      const adapter = dependencyManager.getWorkflowExecutionStorageAdapter();
      expect(adapter).toEqual({ test: "workflowExecutionStorageAdapter" });
    });

    it("should return null when not available", () => {
      const mockContainerNoAdapter = {
        get: vi.fn((id: unknown) => {
          if (id === Identifiers.WorkflowExecutionStorageAdapter) {
            throw new Error("not found");
          }
          return null;
        }),
      } as unknown as Container;

      const mockContextNoAdapter = {
        container: mockContainerNoAdapter,
      } as unknown as GlobalContext;

      const manager = new APIDependencyManager(mockContextNoAdapter);
      const adapter = manager.getWorkflowExecutionStorageAdapter();
      expect(adapter).toBeNull();
    });
  });

  describe("getTaskStorageAdapter", () => {
    it("should return task storage adapter when available", () => {
      const adapter = dependencyManager.getTaskStorageAdapter();
      expect(adapter).toEqual({ test: "taskStorageAdapter" });
    });

    it("should return null when not available", () => {
      const mockContainerNoAdapter = {
        get: vi.fn((id: unknown) => {
          if (id === Identifiers.TaskStorageAdapter) {
            throw new Error("not found");
          }
          return null;
        }),
      } as unknown as Container;

      const mockContextNoAdapter = {
        container: mockContainerNoAdapter,
      } as unknown as GlobalContext;

      const manager = new APIDependencyManager(mockContextNoAdapter);
      const adapter = manager.getTaskStorageAdapter();
      expect(adapter).toBeNull();
    });
  });

  describe("getFileCheckpointManager", () => {
    it("should return file checkpoint manager when available", () => {
      const manager = dependencyManager.getFileCheckpointManager();
      expect(manager).toEqual({ test: "fileCheckpointManager" });
    });

    it("should return undefined when not available", () => {
      const mockContainerNoAdapter = {
        get: vi.fn((id: unknown) => {
          if (id === Identifiers.FileCheckpointManager) {
            throw new Error("not found");
          }
          return null;
        }),
      } as unknown as Container;

      const mockContextNoAdapter = {
        container: mockContainerNoAdapter,
      } as unknown as GlobalContext;

      const manager = new APIDependencyManager(mockContextNoAdapter);
      const fcManager = manager.getFileCheckpointManager();
      expect(fcManager).toBeUndefined();
    });
  });

  describe("getGlobalContext", () => {
    it("should return the global context", () => {
      const context = dependencyManager.getGlobalContext();
      expect(context).toBe(mockGlobalContext);
    });
  });

  describe("getMetricsRegistry", () => {
    it("should return metrics registry from container", () => {
      const registry = dependencyManager.getMetricsRegistry();
      expect(registry).toEqual({ test: "metricsRegistry" });
    });
  });
});
