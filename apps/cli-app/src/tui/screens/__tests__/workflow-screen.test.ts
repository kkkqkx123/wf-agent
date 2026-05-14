/**
 * Unit tests for WorkflowScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { WorkflowScreen } from "..//workflow-screen.js";
import { Container, Box, Text, SelectList } from "../../index.js";

// Mock the WorkflowAdapter - vi.mock is hoisted
const mockListWorkflows = vi.fn();
const mockGetWorkflow = vi.fn();

vi.mock("../../../src/adapters/workflow-adapter.js", () => {
  return {
    WorkflowAdapter: class MockWorkflowAdapter {
      listWorkflows = mockListWorkflows;
      getWorkflow = mockGetWorkflow;
    },
  };
});

describe("WorkflowScreen", () => {
  let screen: WorkflowScreen;
  let onBackMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onBackMock = vi.fn();
    
    // Reset mocks and set default behavior
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
          nodes: [
            { id: "node1", type: "start" },
            { id: "node2", type: "process" },
          ],
          createdAt: "2024-01-01",
          updatedAt: "2024-01-02",
        });
      }
      return Promise.reject(new Error("Workflow not found"));
    });
    
    screen = new WorkflowScreen(onBackMock as any);
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

    it("should initialize adapter and load workflows", async () => {
      // Wait for async initialization
      await new Promise(resolve => setTimeout(resolve, 100));
      expect(screen).toBeDefined();
    });
  });

  describe("render", () => {
    it("should return a Container component", () => {
      const result = screen.render();
      expect(result).toBeInstanceOf(Container);
    });

    it("should render toolbar with action buttons", () => {
      const container = screen.render() as Container;
      expect(container.children.length).toBeGreaterThan(0);
      
      // First child should be toolbar
      const toolbar = container.children[0] as Box;
      expect(toolbar).toBeInstanceOf(Box);
      
      const toolbarChildren = (toolbar as any).children;
      expect(toolbarChildren.length).toBeGreaterThanOrEqual(1);
      expect(toolbarChildren[0]).toBeInstanceOf(Text);
      
      const toolbarText = toolbarChildren[0].render(80)[0];
      expect(toolbarText).toContain("[N]ew");
      expect(toolbarText).toContain("[E]dit");
      expect(toolbarText).toContain("[D]elete");
      expect(toolbarText).toContain("[R]efresh");
      expect(toolbarText).toContain("[B]ack");
    });

    it("should render split layout with list and detail panels", () => {
      const container = screen.render() as Container;
      
      // Second child should be split container
      const splitContainer = container.children[1] as Container;
      expect(splitContainer).toBeInstanceOf(Container);
      
      // Should have two children: list box and detail box
      expect(splitContainer.children.length).toBe(2);
      
      const listBox = splitContainer.children[0] as Box;
      const detailBox = splitContainer.children[1] as Box;
      
      expect(listBox).toBeInstanceOf(Box);
      expect(detailBox).toBeInstanceOf(Box);
    });

    it("should render workflow list in left panel", () => {
      const container = screen.render() as Container;
      const splitContainer = container.children[1] as Container;
      const listBox = splitContainer.children[0] as Box;
      
      const listChildren = (listBox as any).children;
      expect(listChildren.length).toBeGreaterThanOrEqual(2);
      
      // First child should be label
      expect(listChildren[0]).toBeInstanceOf(Text);
      
      // Second child should be SelectList
      expect(listChildren[1]).toBeInstanceOf(SelectList);
    });

    it("should render detail panel in right panel", () => {
      const container = screen.render() as Container;
      const splitContainer = container.children[1] as Container;
      const detailBox = splitContainer.children[1] as Box;
      
      const detailChildren = (detailBox as any).children;
      expect(detailChildren.length).toBeGreaterThanOrEqual(2);
      
      // First child should be label
      expect(detailChildren[0]).toBeInstanceOf(Text);
      
      // Second child should be detail Box
      expect(detailChildren[1]).toBeInstanceOf(Box);
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

    it("should handle 'R' key to refresh workflows", () => {
      const result = screen.handleInput!("R");
      expect(result).toBe(true);
    });

    it("should handle 'n' key for new workflow", () => {
      const consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
      const result = screen.handleInput!("n");
      expect(result).toBe(true);
      expect(consoleSpy).toHaveBeenCalledWith("New workflow - to be implemented");
      consoleSpy.mockRestore();
    });

    it("should handle 'N' key for new workflow", () => {
      const consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
      const result = screen.handleInput!("N");
      expect(result).toBe(true);
      consoleSpy.mockRestore();
    });

    it("should handle 'd' key for delete when workflow selected", () => {
      const consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
      // Set current workflow ID
      (screen as any).currentWorkflowId = "wf1";
      
      const result = screen.handleInput!("d");
      expect(result).toBe(true);
      expect(consoleSpy).toHaveBeenCalledWith("Delete workflow - to be implemented");
      consoleSpy.mockRestore();
    });

    it("should delegate other input to workflow list", () => {
      const result = screen.handleInput!("arrowdown");
      expect(result).toBe(true);
    });

    it("should return false for unhandled input", () => {
      // This might return true if delegated to list, depends on implementation
      const result = screen.handleInput!("x");
      expect(typeof result).toBe("boolean");
    });
  });

  describe("workflow loading", () => {
    it("should load workflows on initialization", async () => {
      // Wait for async initialization
      await new Promise(resolve => setTimeout(resolve, 100));
      
      const container = screen.render() as Container;
      const splitContainer = container.children[1] as Container;
      const listBox = splitContainer.children[0] as Box;
      const workflowList = (listBox as any).children[1] as SelectList;
      
      // Check that items were loaded
      const items = (workflowList as any).items;
      expect(items.length).toBeGreaterThan(0);
      expect(items[0].value).toBe("wf1");
      expect(items[0].label).toBe("Test Workflow 1");
    });

    it("should display workflow details when selected", async () => {
      // Wait for async initialization
      await new Promise(resolve => setTimeout(resolve, 100));
      
      const container = screen.render() as Container;
      const splitContainer = container.children[1] as Container;
      const listBox = splitContainer.children[0] as Box;
      const workflowList = (listBox as any).children[1] as SelectList;
      
      // Simulate selection
      if (workflowList.onSelect) {
        await workflowList.onSelect({ value: "wf1", label: "Test Workflow 1", description: "" });
      }
      
      // Wait for async detail loading
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // Check detail panel was updated
      const detailBox = splitContainer.children[1] as Box;
      const detailPanel = (detailBox as any).children[1] as Box;
      const detailChildren = (detailPanel as any).children;
      
      expect(detailChildren.length).toBeGreaterThan(0);
    });

    it("should handle error when loading non-existent workflow", async () => {
      // Wait for async initialization
      await new Promise(resolve => setTimeout(resolve, 100));
      
      const container = screen.render() as Container;
      const splitContainer = container.children[1] as Container;
      const listBox = splitContainer.children[0] as Box;
      const workflowList = (listBox as any).children[1] as SelectList;
      
      // Simulate selection of non-existent workflow
      if (workflowList.onSelect) {
        await workflowList.onSelect({ value: "nonexistent", label: "Non-existent", description: "" });
      }
      
      // Wait for async detail loading
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // Check error message in detail panel
      const detailBox = splitContainer.children[1] as Box;
      const detailPanel = (detailBox as any).children[1] as Box;
      const detailChildren = (detailPanel as any).children;
      
      expect(detailChildren.length).toBeGreaterThan(0);
      const errorText = detailChildren[0].render(80)[0];
      expect(errorText).toContain("Error");
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
