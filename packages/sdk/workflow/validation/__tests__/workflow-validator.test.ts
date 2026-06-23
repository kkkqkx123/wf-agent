/**
 * Workflow Validator Unit Tests
 * Tests for workflow-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { WorkflowValidator } from "../workflow-validator.js";
import type { WorkflowTemplate } from "@wf-agent/types";

describe("WorkflowValidator", () => {
  const validator = new WorkflowValidator();

  describe("validate", () => {
    it("should validate a valid STANDALONE workflow", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [
          {
            id: "start-1",
            name: "Start",
            type: "START",
            config: {},
          },
          {
            id: "end-1",
            name: "End",
            type: "END",
            config: {},
          },
        ],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validator.validate(workflow);
      expect(result.isOk()).toBe(true);
    });

    it("should fail when workflow has no nodes", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("at least one node"))).toBe(true);
      }
    });

    it("should fail when workflow has duplicate node IDs", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [
          {
            id: "start-1",
            name: "Start",
            type: "START",
            config: {},
          },
          {
            id: "start-1",
            name: "Duplicate Start",
            type: "START",
            config: {},
          },
        ],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("Node ID must be unique"))).toBe(true);
      }
    });

    it("should fail when workflow has no START node", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [
          {
            id: "end-1",
            name: "End",
            type: "END",
            config: {},
          },
        ],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("START node"))).toBe(true);
      }
    });

    it("should fail when workflow has no END node", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [
          {
            id: "start-1",
            name: "Start",
            type: "START",
            config: {},
          },
        ],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("END node"))).toBe(true);
      }
    });
  });
});
