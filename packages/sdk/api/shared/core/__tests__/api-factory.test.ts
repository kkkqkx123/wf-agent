import { describe, it, expect, beforeEach, vi } from "vitest";
import { APIFactory } from "../api-factory.js";
import { APIDependencyManager } from "../sdk-dependencies.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import type { Container } from "@wf-agent/common-utils";

describe("api-factory.ts", () => {
  let mockGlobalContext: GlobalContext;
  let mockContainer: Container;
  let factory: APIFactory;
  let sdkInstances: APIFactory[] = [];

  beforeEach(() => {
    mockContainer = {
      get: vi.fn((_id: unknown) => {
        return {};
      }),
      bind: vi.fn(),
    } as unknown as Container;

    mockGlobalContext = {
      container: mockContainer,
      workflowRegistry: {},
      eventRegistry: {},
      toolRegistry: {},
      llmExecutor: {},
      scriptRegistry: {},
      scriptExecutor: {},
      nodeTemplateRegistry: {},
      triggerTemplateRegistry: {},
      hookTemplateRegistry: {},
    } as unknown as GlobalContext;

    factory = new APIFactory(mockGlobalContext);
    sdkInstances.push(factory);
  });

  afterEach(() => {
    sdkInstances.forEach(f => f.reset());
    sdkInstances = [];
  });

  describe("constructor", () => {
    it("should create APIFactory with dependencies", () => {
      expect(factory).toBeDefined();
    });

    it("should initialize with empty apiInstances cache", () => {
      expect((factory as unknown as { apiInstances: Record<string, unknown> }).apiInstances).toEqual({});
    });
  });

  describe("createAPI (private)", () => {
    it("should create and cache API instance on first call", () => {
      const deps = factory.getDependencies();
      expect(deps).toBeInstanceOf(APIDependencyManager);
    });

    it("should return cached instance on subsequent calls", () => {
      const workflowAPI = factory.createWorkflowAPI();
      const workflowAPI2 = factory.createWorkflowAPI();

      expect(workflowAPI).toBe(workflowAPI2);
    });
  });

  describe("createWorkflowAPI", () => {
    it("should create workflow API", () => {
      const workflowAPI = factory.createWorkflowAPI();
      expect(workflowAPI).toBeDefined();
    });

    it("should return same instance on multiple calls", () => {
      const api1 = factory.createWorkflowAPI();
      const api2 = factory.createWorkflowAPI();
      expect(api1).toBe(api2);
    });
  });

  describe("createToolAPI", () => {
    it("should create tool API", () => {
      const toolAPI = factory.createToolAPI();
      expect(toolAPI).toBeDefined();
    });

    it("should return same instance on multiple calls", () => {
      const api1 = factory.createToolAPI();
      const api2 = factory.createToolAPI();
      expect(api1).toBe(api2);
    });
  });

  describe("createWorkflowExecutionAPI", () => {
    it("should create workflow execution API", () => {
      const execAPI = factory.createWorkflowExecutionAPI();
      expect(execAPI).toBeDefined();
    });
  });

  describe("createScriptAPI", () => {
    it("should create script API", () => {
      const scriptAPI = factory.createScriptAPI();
      expect(scriptAPI).toBeDefined();
    });
  });

  describe("createProfileAPI", () => {
    it("should create profile API", () => {
      const profileAPI = factory.createProfileAPI();
      expect(profileAPI).toBeDefined();
    });
  });

  describe("createNodeTemplateAPI", () => {
    it("should create node template API", () => {
      const api = factory.createNodeTemplateAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createTriggerTemplateAPI", () => {
    it("should create trigger template API", () => {
      const api = factory.createTriggerTemplateAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createUserInteractionAPI", () => {
    it("should create user interaction API", () => {
      const api = factory.createUserInteractionAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createEventAPI", () => {
    it("should create event API with proper eventManager", () => {
      mockGlobalContext = {
        ...mockGlobalContext,
        eventRegistry: {
          on: vi.fn(),
          once: vi.fn(),
          emit: vi.fn(),
          onGlobal: vi.fn(),
          off: vi.fn(),
          removeListener: vi.fn(),
          removeAllListeners: vi.fn(),
        },
      } as unknown as GlobalContext;

      const testFactory = new APIFactory(mockGlobalContext);
      sdkInstances.push(testFactory);

      const api = testFactory.createEventAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createTriggerAPI", () => {
    it("should create trigger API", () => {
      const api = factory.createTriggerAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createVariableAPI", () => {
    it("should create variable API", () => {
      const api = factory.createVariableAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createMessageAPI", () => {
    it("should create message API", () => {
      const api = factory.createMessageAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createSkillAPI", () => {
    it("should create skill API", () => {
      const api = factory.createSkillAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createMetricsAPI", () => {
    it("should create metrics API", () => {
      const api = factory.createMetricsAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createTaskAPI", () => {
    it("should create task API", () => {
      const api = factory.createTaskAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createWorkflowGraphQueryAPI", () => {
    it("should create workflow graph query API", () => {
      const api = factory.createWorkflowGraphQueryAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createStorageDiagnosticsAPI", () => {
    it("should create storage diagnostics API", () => {
      const api = factory.createStorageDiagnosticsAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createSearchAPI", () => {
    it("should create search API", () => {
      const api = factory.createSearchAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createFileCheckpointAPI", () => {
    it("should create file checkpoint API", () => {
      const api = factory.createFileCheckpointAPI();
      expect(api).toBeDefined();
    });
  });

  describe("createAllAPIs", () => {
    it("should create all API instances", () => {
      const testFactory = new APIFactory({
        ...mockGlobalContext,
        eventRegistry: {
          on: vi.fn(),
          once: vi.fn(),
          emit: vi.fn(),
          onGlobal: vi.fn(),
          off: vi.fn(),
          removeListener: vi.fn(),
          removeAllListeners: vi.fn(),
        },
      } as unknown as GlobalContext);
      sdkInstances.push(testFactory);

      const apis = testFactory.createAllAPIs();

      expect(apis).toBeDefined();
      expect(apis.workflows).toBeDefined();
      expect(apis.tools).toBeDefined();
      expect(apis.executions).toBeDefined();
      expect(apis.scripts).toBeDefined();
      expect(apis.profiles).toBeDefined();
      expect(apis.nodeTemplates).toBeDefined();
      expect(apis.triggerTemplates).toBeDefined();
      expect(apis.userInteractions).toBeDefined();
      expect(apis.events).toBeDefined();
      expect(apis.triggers).toBeDefined();
      expect(apis.variables).toBeDefined();
      expect(apis.messages).toBeDefined();
      expect(apis.skills).toBeDefined();
      expect(apis.metrics).toBeDefined();
      expect(apis.tasks).toBeDefined();
      expect(apis.graphs).toBeDefined();
      expect(apis.diagnostics).toBeDefined();
      expect(apis.search).toBeDefined();
      expect(apis.fileCheckpoints).toBeDefined();
    });

    it("should return cached instances", () => {
      const testFactory = new APIFactory({
        ...mockGlobalContext,
        eventRegistry: {
          on: vi.fn(),
          once: vi.fn(),
          emit: vi.fn(),
          onGlobal: vi.fn(),
          off: vi.fn(),
          removeListener: vi.fn(),
          removeAllListeners: vi.fn(),
        },
      } as unknown as GlobalContext);
      sdkInstances.push(testFactory);

      const apis1 = testFactory.createAllAPIs();
      const apis2 = testFactory.createAllAPIs();

      expect(apis1.workflows).toBe(apis2.workflows);
      expect(apis1.tools).toBe(apis2.tools);
      expect(apis1.executions).toBe(apis2.executions);
    });
  });

  describe("getDependencies", () => {
    it("should return the dependency manager", () => {
      const deps = factory.getDependencies();
      expect(deps).toBeInstanceOf(APIDependencyManager);
    });

    it("should return same dependency manager on multiple calls", () => {
      const deps1 = factory.getDependencies();
      const deps2 = factory.getDependencies();
      expect(deps1).toBe(deps2);
    });
  });

  describe("reset", () => {
    it("should clear all cached API instances", () => {
      const workflowAPI1 = factory.createWorkflowAPI();
      const toolAPI1 = factory.createToolAPI();

      factory.reset();

      const workflowAPI2 = factory.createWorkflowAPI();
      const toolAPI2 = factory.createToolAPI();

      expect(workflowAPI1).not.toBe(workflowAPI2);
      expect(toolAPI1).not.toBe(toolAPI2);
    });
  });
});
