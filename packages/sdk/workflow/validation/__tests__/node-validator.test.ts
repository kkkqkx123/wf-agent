/**
 * Node Validator Unit Tests
 * Tests for node-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { NodeValidator } from "../node-validator.js";

describe("NodeValidator", () => {
  const validator = new NodeValidator();

  describe("validateNode", () => {
    it("should validate a valid START node", () => {
      const node = {
        id: "start-1",
        name: "Start Node",
        type: "START",
        config: {},
      } as any;

      const result = validator.validateNode(node);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid END node", () => {
      const node = {
        id: "end-1",
        name: "End Node",
        type: "END",
        config: {},
      } as any;

      const result = validator.validateNode(node);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid SCRIPT node", () => {
      const node = {
        id: "script-1",
        name: "Script Node",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
        },
      } as any;

      const result = validator.validateNode(node);
      expect(result.isOk()).toBe(true);
    });
  });

  describe("validateNodes", () => {
    it("should validate multiple nodes", () => {
      const nodes = [
        {
          id: "start-1",
          name: "Start Node",
          type: "START",
          config: {},
        },
        {
          id: "end-1",
          name: "End Node",
          type: "END",
          config: {},
        },
      ] as any[];

      const results = validator.validateNodes(nodes);
      expect(results).toHaveLength(2);
      expect(results[0]!.isOk()).toBe(true);
      expect(results[1]!.isOk()).toBe(true);
    });

    it("should return errors for invalid nodes", () => {
      const nodes = [
        {
          id: "start-1",
          name: "Start Node",
          type: "START",
          config: {},
        },
        {
          id: "fork-1",
          name: "Fork Node",
          type: "FORK",
          config: {
            forkPaths: [],
          },
        },
      ] as any[];

      const results = validator.validateNodes(nodes);
      expect(results).toHaveLength(2);
      expect(results[0]!.isOk()).toBe(true);
      expect(results[1]!.isErr()).toBe(true);
    });
  });

  describe("validateRawNode", () => {
    it("should validate a valid raw node", () => {
      const rawNode = {
        id: "node-1",
        name: "Node 1",
        type: "START",
        config: {},
        outgoingEdgeIds: [],
        incomingEdgeIds: [],
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isOk()).toBe(true);
    });

    it("should fail when node is null", () => {
      const result = validator.validateRawNode(null);
      expect(result.isErr()).toBe(true);
    });

    it("should fail when node is not an object", () => {
      const result = validator.validateRawNode("not an object");
      expect(result.isErr()).toBe(true);
    });

    it("should fail when id is missing", () => {
      const rawNode = {
        name: "Node 1",
        type: "START",
        config: {},
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("id is required");
      }
    });

    it("should fail when id is empty", () => {
      const rawNode = {
        id: "",
        name: "Node 1",
        type: "START",
        config: {},
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
    });

    it("should fail when name is missing", () => {
      const rawNode = {
        id: "node-1",
        type: "START",
        config: {},
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("name is required");
      }
    });

    it("should fail when type is invalid", () => {
      const rawNode = {
        id: "node-1",
        name: "Node 1",
        type: "INVALID_TYPE",
        config: {},
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("Invalid node type");
      }
    });

    it("should fail when config is missing", () => {
      const rawNode = {
        id: "node-1",
        name: "Node 1",
        type: "START",
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("config is required");
      }
    });

    it("should fail when outgoingEdgeIds is not an array", () => {
      const rawNode = {
        id: "node-1",
        name: "Node 1",
        type: "START",
        config: {},
        outgoingEdgeIds: "not-an-array",
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("outgoingEdgeIds must be an array");
      }
    });

    it("should fail when incomingEdgeIds is not an array", () => {
      const rawNode = {
        id: "node-1",
        name: "Node 1",
        type: "START",
        config: {},
        incomingEdgeIds: "not-an-array",
      };

      const result = validator.validateRawNode(rawNode);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]!.message).toContain("incomingEdgeIds must be an array");
      }
    });
  });
});
