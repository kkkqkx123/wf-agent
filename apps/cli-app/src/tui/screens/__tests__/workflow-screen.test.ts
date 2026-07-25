/**
 * Unit tests for WorkflowScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { WorkflowScreen } from "..//workflow-screen.js";
import { Container, Text, SelectList } from "../../index.js";

const mockListWorkflows = vi.fn();
const mockGetWorkflow = vi.fn();

vi.mock("../../../adapters/workflow-adapter.js", () => ({
  WorkflowAdapter: class MockWorkflowAdapter {
    listWorkflows = mockListWorkflows;
    getWorkflow = mockGetWorkflow;
    deleteWorkflow = vi.fn().mockResolvedValue(undefined);
  },
}));

vi.mock("../../../adapters/workflow-graph-adapter.js", () => ({
  WorkflowGraphAdapter: class MockWorkflowGraphAdapter {
    getNodes = vi.fn().mockResolvedValue([]);
    getEdges = vi.fn().mockResolvedValue([]);
  },
}));

describe("WorkflowScreen", () => {
  let screen: WorkflowScreen;
  let onBackMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onBackMock = vi.fn();

    mockListWorkflows.mockReset();
    mockListWorkflows.mockResolvedValue([
      { id: "wf1", name: "Test Workflow 1", version: "1.0", status: "active" },
      { id: "wf2", name: "Test Workflow 2", version: "2.0", status: "inactive" },
    ]);

    mockGetWorkflow.mockReset();
    mockGetWorkflow.mockImplementation((id: string) => {
      if (id === "wf1") {
        return Promise.resolve({
          id: "wf1",
          name: "Test Workflow 1",
          version: "1.0",
          description: "A test workflow",
          createdAt: "2024-01-01",
        });
      }
      return Promise.reject(new Error("Workflow not found"));
    });

    screen = new WorkflowScreen(onBackMock);
  });

  describe("constructor", () => {
    it("should create instance without onBack callback", () => {
      const screenWithoutCallback = new WorkflowScreen();
      expect(screenWithoutCallback).toBeDefined();
      expect(screenWithoutCallback.render()).toBeDefined();
    });

    it("should create instance with onBack callback", () => {
      expect(screen).toBeDefined();
      expect(onBackMock).toBeDefined();
    });
  });

  describe("render", () => {
    it("should return a Container component", () => {
      const result = screen.render();
      expect(result).toBeInstanceOf(Container);
    });

    it("should render workflow list as first child", () => {
      const container = screen.render() as Container;
      const list = container.children[0] as SelectList;
      expect(list).toBeInstanceOf(SelectList);
    });
  });

  describe("handleInput", () => {
    it("should handle 'b' key to go back", () => {
      const result = screen.handleInput!("b");
      expect(result).toBe(true);
      expect(onBackMock).toHaveBeenCalled();
    });

    it("should handle 'B' key to go back", () => {
      const result = screen.handleInput!("B");
      expect(result).toBe(true);
      expect(onBackMock).toHaveBeenCalled();
    });

    it("should handle 'r' key to refresh workflows", () => {
      const result = screen.handleInput!("r");
      expect(result).toBe(true);
    });

    it("should handle 'n' key for new workflow", () => {
      const result = screen.handleInput!("n");
      expect(result).toBe(true);
    });

    it("should delegate list navigation to SelectList", () => {
      const result = screen.handleInput!("arrowdown");
      expect(result).toBe(true);
    });
  });

  describe("workflow loading", () => {
    it("should load workflows on initialization", async () => {
      await new Promise(resolve => setTimeout(resolve, 100));

      const container = screen.render() as Container;
      const list = container.children[0] as SelectList;
      const items = (list as any).items;

      expect(items.length).toBeGreaterThan(0);
      expect(items[0].value).toBe("wf1");
    });
  });

  describe("destroy", () => {
    it("should have destroy method", () => {
      expect(typeof screen.destroy).toBe("function");
    });

    it("should cleanup without errors", () => {
      expect(() => screen.destroy!()).not.toThrow();
    });
  });

  describe("Screen interface compliance", () => {
    it("should implement render method", () => {
      expect(typeof screen.render).toBe("function");
    });

    it("should implement handleInput method", () => {
      expect(typeof screen.handleInput).toBe("function");
    });

    it("should implement destroy method", () => {
      expect(typeof screen.destroy).toBe("function");
    });
  });
});
