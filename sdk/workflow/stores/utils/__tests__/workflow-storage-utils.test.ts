/**
 * Tests for workflow storage utilities
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import {
  persistWorkflow,
  removeWorkflow,
  loadWorkflow,
  initializeWorkflowsFromStorage,
} from "../workflow-storage-utils.js";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import type { WorkflowTemplate } from "@wf-agent/types";

vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("@wf-agent/common-utils", async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    getErrorMessage: (error: unknown) =>
      error instanceof Error ? error.message : String(error),
  };
});

function createMockWorkflow(id: string): WorkflowTemplate {
  return {
    id,
    name: `Workflow ${id}`,
    type: "WORKFLOW",
    version: "1.0",
    nodes: [],
    edges: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
    metadata: {
      tags: ["test"],
      category: "test-category",
      author: "test-author",
    },
  } as unknown as WorkflowTemplate;
}

describe("workflow-storage-utils", () => {
  let mockAdapter: WorkflowStorageAdapter;

  beforeEach(() => {
    mockAdapter = {
      save: vi.fn(),
      delete: vi.fn(),
      load: vi.fn(),
      list: vi.fn(),
    } as unknown as WorkflowStorageAdapter;
  });

  describe("persistWorkflow", () => {
    it("should persist workflow to storage", async () => {
      const workflow = createMockWorkflow("wf-1");

      await persistWorkflow(workflow, mockAdapter);

      expect(mockAdapter.save).toHaveBeenCalledWith(
        "wf-1",
        expect.any(Uint8Array),
        expect.objectContaining({
          workflowId: "wf-1",
          name: "Workflow wf-1",
        }),
      );
    });

    it("should skip persistence when no adapter", async () => {
      const workflow = createMockWorkflow("wf-1");
      await persistWorkflow(workflow, null);
      // Should not throw
    });

    it("should skip persistence when adapter is undefined", async () => {
      const workflow = createMockWorkflow("wf-1");
      await persistWorkflow(workflow, undefined);
      // Should not throw
    });

    it("should throw on storage error", async () => {
      const workflow = createMockWorkflow("wf-1");
      (mockAdapter.save as Mock).mockRejectedValue(new Error("Storage error"));

      await expect(persistWorkflow(workflow, mockAdapter)).rejects.toThrow("Storage error");
    });
  });

  describe("removeWorkflow", () => {
    it("should remove workflow from storage", async () => {
      await removeWorkflow("wf-1", mockAdapter);

      expect(mockAdapter.delete).toHaveBeenCalledWith("wf-1");
    });

    it("should skip removal when no adapter", async () => {
      await removeWorkflow("wf-1", null);
      // Should not throw
    });

    it("should skip removal when adapter is undefined", async () => {
      await removeWorkflow("wf-1", undefined);
      // Should not throw
    });

    it("should handle storage error gracefully", async () => {
      (mockAdapter.delete as Mock).mockRejectedValue(new Error("Storage error"));

      await expect(removeWorkflow("wf-1", mockAdapter)).resolves.not.toThrow();
    });
  });

  describe("loadWorkflow", () => {
    it("should load workflow from storage", async () => {
      const workflow = createMockWorkflow("wf-1");
      const encoder = new TextEncoder();
      const data = encoder.encode(JSON.stringify(workflow));
      (mockAdapter.load as Mock).mockResolvedValue(data);

      const result = await loadWorkflow("wf-1", mockAdapter);

      expect(result).toEqual(workflow);
    });

    it("should return null when no data in storage", async () => {
      (mockAdapter.load as Mock).mockResolvedValue(null);

      const result = await loadWorkflow("wf-1", mockAdapter);

      expect(result).toBeNull();
    });

    it("should return null when no adapter", async () => {
      const result = await loadWorkflow("wf-1", null);
      expect(result).toBeNull();
    });

    it("should return null on storage error", async () => {
      (mockAdapter.load as Mock).mockRejectedValue(new Error("Storage error"));

      const result = await loadWorkflow("wf-1", mockAdapter);

      expect(result).toBeNull();
    });
  });

  describe("initializeWorkflowsFromStorage", () => {
    it("should load all workflows from storage into map", async () => {
      const encoder = new TextEncoder();
      const wf1 = createMockWorkflow("wf-1");
      const wf2 = createMockWorkflow("wf-2");

      (mockAdapter.list as Mock).mockResolvedValue(["wf-1", "wf-2"]);
      (mockAdapter.load as Mock)
        .mockResolvedValueOnce(encoder.encode(JSON.stringify(wf1)))
        .mockResolvedValueOnce(encoder.encode(JSON.stringify(wf2)));

      const workflows = new Map<string, WorkflowTemplate>();
      await initializeWorkflowsFromStorage(mockAdapter, workflows);

      expect(workflows.size).toBe(2);
      expect(workflows.get("wf-1")).toEqual(wf1);
      expect(workflows.get("wf-2")).toEqual(wf2);
    });

    it("should skip when no adapter", async () => {
      const workflows = new Map<string, WorkflowTemplate>();
      await initializeWorkflowsFromStorage(null, workflows);
      expect(workflows.size).toBe(0);
    });

    it("should handle storage list error gracefully", async () => {
      (mockAdapter.list as Mock).mockRejectedValue(new Error("Storage error"));

      const workflows = new Map<string, WorkflowTemplate>();
      await initializeWorkflowsFromStorage(mockAdapter, workflows);

      expect(workflows.size).toBe(0);
    });

    it("should handle individual load errors without affecting others", async () => {
      const encoder = new TextEncoder();
      const wf1 = createMockWorkflow("wf-1");

      (mockAdapter.list as Mock).mockResolvedValue(["wf-1", "wf-2"]);
      (mockAdapter.load as Mock)
        .mockResolvedValueOnce(encoder.encode(JSON.stringify(wf1)))
        .mockRejectedValueOnce(new Error("Load error for wf-2"));

      const workflows = new Map<string, WorkflowTemplate>();
      await initializeWorkflowsFromStorage(mockAdapter, workflows);

      expect(workflows.size).toBe(1);
      expect(workflows.get("wf-1")).toEqual(wf1);
    });
  });
});