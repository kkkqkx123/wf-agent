/**
 * Workflow Config Validation Unit Tests
 * Tests for workflow-config-validation.ts functionality
 */

import { describe, it, expect } from "vitest";
import {
  validateWorkflowConfig,
  getWorkflowValidationWarnings,
} from "../workflow-config-validation.js";
import type { WorkflowTemplate } from "@wf-agent/types";

describe("validateWorkflowConfig", () => {
  describe("valid workflows", () => {
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

      const result = validateWorkflowConfig(workflow);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid DEPENDENT workflow with SUBGRAPH node", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-2",
        name: "Dependent Workflow",
        type: "DEPENDENT",
        version: "1.0.0",
        nodes: [
          {
            id: "start-1",
            name: "Start",
            type: "START",
            config: {},
          },
          {
            id: "subgraph-1",
            name: "Subgraph",
            type: "SUBGRAPH",
            config: {
              subgraphId: "child-workflow",
              async: false,
            },
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

      const result = validateWorkflowConfig(workflow);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid TRIGGERED_SUBWORKFLOW", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-3",
        name: "Triggered Subworkflow",
        type: "TRIGGERED_SUBWORKFLOW",
        version: "1.0.0",
        nodes: [
          {
            id: "start-trigger",
            name: "Start From Trigger",
            type: "START_FROM_TRIGGER",
            config: {},
          },
          {
            id: "continue-trigger",
            name: "Continue From Trigger",
            type: "CONTINUE_FROM_TRIGGER",
            config: {},
          },
        ],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validateWorkflowConfig(workflow);
      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid workflows", () => {
    it("should fail validation when id is missing", () => {
      const workflow = {
        id: "",
        name: "Test Workflow",
        type: "STANDALONE",
        version: "1.0.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as WorkflowTemplate;

      const result = validateWorkflowConfig(workflow);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when type is invalid", () => {
      const workflow = {
        id: "workflow-1",
        name: "Test Workflow",
        type: "INVALID_TYPE",
        version: "1.0.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as unknown as WorkflowTemplate;

      const result = validateWorkflowConfig(workflow);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when STANDALONE workflow has SUBGRAPH node", () => {
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
            id: "subgraph-1",
            name: "Subgraph",
            type: "SUBGRAPH",
            config: {
              subgraphId: "child-workflow",
              async: false,
            },
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

      const result = validateWorkflowConfig(workflow);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when DEPENDENT workflow has no SUBGRAPH node", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-1",
        name: "Dependent Workflow",
        type: "DEPENDENT",
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

      const result = validateWorkflowConfig(workflow);
      expect(result.isErr()).toBe(true);
    });
  });
});

describe("getWorkflowValidationWarnings", () => {
  it("should return empty array for small workflow", () => {
    const workflow: WorkflowTemplate = {
      id: "workflow-1",
      name: "Small Workflow",
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

    const warnings = getWorkflowValidationWarnings(workflow);
    expect(warnings).toHaveLength(0);
  });

  it("should return warning for large workflow", () => {
    const nodes = [];
    for (let i = 0; i < 51; i++) {
      nodes.push({
        id: `node-${i}`,
        name: `Node ${i}`,
        type: "SCRIPT" as const,
        config: {
          scriptName: `script-${i}`,
          risk: "low" as const,
        },
      });
    }

    const workflow: WorkflowTemplate = {
      id: "workflow-1",
      name: "Large Workflow",
      type: "STANDALONE",
      version: "1.0.0",
      nodes,
      edges: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    const warnings = getWorkflowValidationWarnings(workflow);
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain("many nodes");
  });
});
