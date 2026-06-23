/**
 * Mock Container factory for sdk/shared integration tests
 * Creates a DI container with mocked services
 */

import { Container } from "@wf-agent/common-utils";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import * as Identifiers from "@sdk/di/service-identifiers";
import { vi } from "vitest";

export function createMockContainer(config?: { name?: string }): Container {
  const container = new Container();

  // Create mock services
  const createMockRegistry = () => ({
    get: vi.fn(),
    register: vi.fn(),
    getAll: vi.fn(() => []),
    keys: vi.fn(() => []),
    has: vi.fn(() => false),
    delete: vi.fn(),
  });

  const createMockExecutor = () => ({
    execute: vi.fn(),
    executeStream: vi.fn(),
  });

  // Register mock services
  const services: Record<string, any> = {
    [Identifiers.WorkflowRegistry]: createMockRegistry(),
    [Identifiers.ToolRegistry]: createMockRegistry(),
    [Identifiers.ScriptRegistry]: createMockRegistry(),
    [Identifiers.EventRegistry]: {
      get: vi.fn(),
      emit: vi.fn(),
      on: vi.fn(),
      off: vi.fn(),
      getAllListeners: vi.fn(() => []),
    },
    [Identifiers.NodeTemplateRegistry]: createMockRegistry(),
    [Identifiers.TriggerTemplateRegistry]: createMockRegistry(),
    [Identifiers.HookTemplateRegistry]: createMockRegistry(),
    [Identifiers.PromptTemplateRegistry]: createMockRegistry(),
    [Identifiers.FragmentRegistry]: createMockRegistry(),
    [Identifiers.LLMExecutor]: createMockExecutor(),
    [Identifiers.ToolCallExecutor]: createMockExecutor(),
    [Identifiers.MetricsRegistry]: {
      recordMetric: vi.fn(),
      getMetrics: vi.fn(() => []),
    },
  };

  // Register all mocks
  for (const [key, value] of Object.entries(services)) {
    container.register(key as ServiceIdentifier<any>, { useValue: value });
  }

  return container;
}

/**
 * Get all registered mock services from a container
 */
export function getMockServices(container: Container) {
  return {
    workflowRegistry: container.get(Identifiers.WorkflowRegistry),
    toolRegistry: container.get(Identifiers.ToolRegistry),
    scriptRegistry: container.get(Identifiers.ScriptRegistry),
    eventRegistry: container.get(Identifiers.EventRegistry),
    nodeTemplateRegistry: container.get(Identifiers.NodeTemplateRegistry),
    triggerTemplateRegistry: container.get(Identifiers.TriggerTemplateRegistry),
    llmExecutor: container.get(Identifiers.LLMExecutor),
    toolCallExecutor: container.get(Identifiers.ToolCallExecutor),
  };
}

/**
 * Reset all mock calls
 */
export function resetAllMocks(container: Container) {
  const services = getMockServices(container);
  for (const service of Object.values(services)) {
    if (typeof service?.mockReset === "function") {
      service.mockReset();
    }
    // Reset all mock functions in the service
    for (const [key, value] of Object.entries(service)) {
      if (typeof value?.mockReset === "function") {
        value.mockReset();
      }
    }
  }
}
