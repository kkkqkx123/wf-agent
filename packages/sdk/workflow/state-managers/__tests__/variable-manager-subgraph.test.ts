/**
 * VariableManager - SUBGRAPH Variable Passing Unit Tests
 * Tests for importVariables and exportVariables functionality used by SUBGRAPH nodes
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { VariableManager } from "../../execution/utils/variable-manager.js";
import { RuntimeValidationError } from "@wf-agent/types";

// Mock structuredClone
const mockStructuredClone = vi.fn();
vi.stubGlobal("structuredClone", mockStructuredClone);

describe("VariableManager - SUBGRAPH Variable Passing", () => {
  let parentManager: VariableManager;
  let childManager: VariableManager;

  beforeEach(() => {
    vi.clearAllMocks();
    mockStructuredClone.mockImplementation(value => {
      // Simple deep clone for testing
      return JSON.parse(JSON.stringify(value));
    });

    parentManager = new VariableManager();
    childManager = new VariableManager();

    // Setup parent variables - need to register them first
    parentManager.registerVariable({
      name: "parentVar1",
      type: "string",
      value: "value1",
      readonly: false,
    });
    parentManager.registerVariable({
      name: "parentVar2",
      type: "object",
      value: { nested: { data: "test" } },
      readonly: false,
    });
    parentManager.registerVariable({
      name: "parentVar3",
      type: "array",
      value: [1, 2, 3],
      readonly: false,
    });
  });

  describe("importVariables", () => {
    it("should import required variable successfully with deep clone", () => {
      // Arrange
      const mappings = [{ sourcePath: "parentVar1", internalName: "childVar1", required: true }];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childVar1")).toBe("value1");
      expect(mockStructuredClone).toHaveBeenCalledWith("value1");
    });

    it("should import object variable with deep clone to prevent mutation", () => {
      // Arrange
      const originalObject = { nested: { data: "test" } };
      const mappings = [{ sourcePath: "parentVar2", internalName: "childVar2", required: true }];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      const importedValue = childManager.getVariable("childVar2");
      expect(importedValue).toEqual(originalObject);
      expect(importedValue).not.toBe(originalObject); // Should be a clone

      // Verify mutation doesn't affect parent
      (importedValue as any).nested.data = "modified";
      expect(parentManager.getVariable("parentVar2")).toEqual({ nested: { data: "test" } });
    });

    it("should throw error when required variable is missing", () => {
      // Arrange
      const mappings = [
        { sourcePath: "nonExistentVar", internalName: "childVar", required: true },
      ];

      // Act & Assert
      expect(() => {
        childManager.importVariables(parentManager, mappings);
      }).toThrow(RuntimeValidationError);

      expect(() => {
        childManager.importVariables(parentManager, mappings);
      }).toThrow("Required input variable 'nonExistentVar' not found in parent workflow");
    });

    it("should use default value for optional missing variable", () => {
      // Arrange
      const defaultValue = { default: "value" };
      const mappings = [
        {
          sourcePath: "nonExistentVar",
          internalName: "childVar",
          required: false,
          defaultValue,
        },
      ];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childVar")).toEqual(defaultValue);
      expect(childManager.getVariable("childVar")).not.toBe(defaultValue); // Should be cloned
    });

    it("should skip optional variable without default value", () => {
      // Arrange
      const mappings = [
        { sourcePath: "nonExistentVar", internalName: "childVar", required: false },
      ];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childVar")).toBeUndefined();
    });

    it("should handle structuredClone failure gracefully", () => {
      // Arrange
      const circularReference: any = { data: "test" };
      circularReference.self = circularReference; // Circular reference

      parentManager.registerVariable({
        name: "circularVar",
        type: "object",
        value: circularReference,
        readonly: false,
      });

      // Mock structuredClone to fail
      mockStructuredClone.mockImplementation(() => {
        throw new Error("structuredClone failed");
      });

      const mappings = [{ sourcePath: "circularVar", internalName: "childVar", required: true }];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childVar")).toBe(circularReference);
      // Should fall back to shallow copy
    });

    it("should import multiple variables with mixed requirements", () => {
      // Arrange
      const mappings = [
        { sourcePath: "parentVar1", internalName: "childVar1", required: true },
        { sourcePath: "parentVar2", internalName: "childVar2", required: false },
        {
          sourcePath: "nonExistentVar",
          internalName: "childVar3",
          required: false,
          defaultValue: "default",
        },
      ];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childVar1")).toBe("value1");
      expect(childManager.getVariable("childVar2")).toEqual({ nested: { data: "test" } });
      expect(childManager.getVariable("childVar3")).toBe("default");
    });

    it("should import using sourcePath for nested variable access", () => {
      // Arrange
      const mappings = [
        {
          sourcePath: "parentVar2.nested.data",
          internalName: "childNested",
          required: true,
        },
      ];

      // Act
      childManager.importVariables(parentManager, mappings);

      // Assert
      expect(childManager.getVariable("childNested")).toBe("test");
    });

  });

  describe("resolvePath", () => {
    it("should resolve simple path via getVariable", () => {
      // Act
      const value = parentManager.resolvePath("parentVar1");

      // Assert
      expect(value).toBe("value1");
    });

    it("should resolve nested path from serialized variables", () => {
      // Act
      const value = parentManager.resolvePath("parentVar2.nested.data");

      // Assert
      expect(value).toBe("test");
    });

    it("should return undefined for non-existent path", () => {
      // Act
      const value = parentManager.resolvePath("nonExistent.field");

      // Assert
      expect(value).toBeUndefined();
    });

    it("should return undefined for undefined path", () => {
      // Act
      const value = parentManager.resolvePath("");

      // Assert
      expect(value).toBeUndefined();
    });
  });

  describe("setPath", () => {
    it("should set simple path via setVariable (existing variable)", () => {
      // Arrange
      parentManager.registerVariable({
        name: "mutableVar",
        type: "string",
        value: "old",
        readonly: false,
      });

      // Act
      parentManager.setPath("mutableVar", "new");

      // Assert
      expect(parentManager.getVariable("mutableVar")).toBe("new");
    });

    it("should set simple path by creating new variable", () => {
      // Act
      parentManager.setPath("newSimpleVar", "newValue");

      // Assert
      expect(parentManager.getVariable("newSimpleVar")).toBe("newValue");
    });

    it("should set nested path with existing root object", () => {
      // Arrange
      parentManager.registerVariable({
        name: "config",
        type: "object",
        value: { existing: "keep" },
        readonly: false,
      });

      // Act
      parentManager.setPath("config.newField", "added");

      // Assert
      const config = parentManager.getVariable("config") as Record<string, unknown>;
      expect(config.existing).toBe("keep");
      expect(config.newField).toBe("added");
    });

    it("should create root object when setting nested path on non-existent root", () => {
      // Act
      parentManager.setPath("newRoot.nested.field", "deepValue");

      // Assert
      expect(parentManager.getVariable("newRoot")).toBeDefined();
      const root = parentManager.getVariable("newRoot") as Record<string, unknown>;
      expect((root.nested as Record<string, unknown>).field).toBe("deepValue");
    });
  });

  describe("exportVariables with targetPath", () => {
    beforeEach(() => {
      // Setup child variables
      childManager.registerVariable({
        name: "childResult",
        type: "string",
        value: "success",
        readonly: false,
      });
      childManager.registerVariable({
        name: "childData",
        type: "object",
        value: { result: "data" },
        readonly: false,
      });
    });

    it("should export variable successfully with deep clone", () => {
      // Arrange
      const mappings = [{ internalName: "childResult", targetPath: "parentResult" }];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      expect(parentManager.getVariable("parentResult")).toBe("success");
      expect(mockStructuredClone).toHaveBeenCalledWith("success");
    });

    it("should export object variable with deep clone to prevent mutation", () => {
      // Arrange
      const originalObject = { result: "data" };
      const mappings = [{ internalName: "childData", targetPath: "parentData" }];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      const exportedValue = parentManager.getVariable("parentData");
      expect(exportedValue).toEqual(originalObject);
      expect(exportedValue).not.toBe(originalObject); // Should be a clone

      // Verify mutation doesn't affect child
      (exportedValue as any).result = "modified";
      expect(childManager.getVariable("childData")).toEqual({ result: "data" });
    });

    it("should skip undefined output variable (optional output)", () => {
      // Arrange
      const mappings = [{ internalName: "nonExistentVar", targetPath: "parentOutput" }];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      expect(parentManager.getVariable("parentOutput")).toBeUndefined();
      expect(mockStructuredClone).not.toHaveBeenCalled();
    });

    it("should handle structuredClone failure during export gracefully", () => {
      // Arrange
      const circularReference: any = { data: "test" };
      circularReference.self = circularReference; // Circular reference

      childManager.registerVariable({
        name: "circularResult",
        type: "object",
        value: circularReference,
        readonly: false,
      });

      // Mock structuredClone to fail
      mockStructuredClone.mockImplementation(() => {
        throw new Error("structuredClone failed");
      });

      const mappings = [{ internalName: "circularResult", targetPath: "parentResult" }];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      expect(parentManager.getVariable("parentResult")).toBe(circularReference);
      // Should fall back to shallow copy
    });

    it("should export multiple variables", () => {
      // Arrange
      const mappings = [
        { internalName: "childResult", targetPath: "parentResult" },
        { internalName: "childData", targetPath: "parentData" },
        { internalName: "nonExistentVar", targetPath: "parentOptional" }, // Should be skipped
      ];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      expect(parentManager.getVariable("parentResult")).toBe("success");
      expect(parentManager.getVariable("parentData")).toEqual({ result: "data" });
      expect(parentManager.getVariable("parentOptional")).toBeUndefined();
    });

    it("should export to nested path using targetPath", () => {
      // Arrange
      parentManager.registerVariable({
        name: "output",
        type: "object",
        value: {},
        readonly: false,
      });
      const mappings = [
        { internalName: "childResult", targetPath: "output.status" },
      ];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      const output = parentManager.getVariable("output") as Record<string, unknown>;
      expect(output.status).toBe("success");
    });

    it("should export to simple path using targetPath", () => {
      // Arrange
      const mappings = [{ internalName: "childResult", targetPath: "simpleResult" }];

      // Act
      childManager.exportVariables(parentManager, mappings);

      // Assert
      expect(parentManager.getVariable("simpleResult")).toBe("success");
    });
  });

  describe("integration - full SUBGRAPH variable flow", () => {
    it("should complete full import-execute-export cycle with isolation", () => {
      // Arrange
      const inputMappings = [
        { sourcePath: "parentInput", internalName: "childInput", required: true },
      ];

      const outputMappings = [{ internalName: "childOutput", targetPath: "parentOutput" }];

      // Setup parent input
      const originalInput = { data: "original", nested: { value: 1 } };
      parentManager.registerVariable({
        name: "parentInput",
        type: "object",
        value: originalInput,
        readonly: false,
      });

      // Act - Import phase
      childManager.importVariables(parentManager, inputMappings);

      // Simulate child workflow execution (modify the imported data)
      const childInput = childManager.getVariable("childInput") as any;
      childInput.data = "modified";
      childInput.nested.value = 2;

      // Set child output
      const childOutput = { result: "processed", input: childInput };
      childManager.registerVariable({
        name: "childOutput",
        type: "object",
        value: childOutput,
        readonly: false,
      });

      // Export phase
      childManager.exportVariables(parentManager, outputMappings);

      // Assert
      const exportedOutput = parentManager.getVariable("parentOutput") as any;

      // Verify parent input was not mutated by child
      expect(parentManager.getVariable("parentInput")).toEqual({
        data: "original",
        nested: { value: 1 },
      });

      // Verify child output was exported
      expect(exportedOutput.result).toBe("processed");

      // Verify exported output is a clone, not the same object
      expect(exportedOutput).not.toBe(childOutput);

      // Verify exported output contains modified child data
      expect(exportedOutput.input.data).toBe("modified");
      expect(exportedOutput.input.nested.value).toBe(2);
    });

    it("should complete full import-execute-export cycle with path-based mappings", () => {
      // Arrange: use sourcePath for import and targetPath for export
      parentManager.registerVariable({
        name: "config",
        type: "object",
        value: { user: { name: "Alice", role: "admin" } },
        readonly: false,
      });
      parentManager.registerVariable({
        name: "resultBucket",
        type: "object",
        value: {},
        readonly: false,
      });

      const inputMappings = [
        {
          sourcePath: "config.user.name",
          internalName: "childName",
          required: true,
        },
      ];

      const outputMappings = [
        { internalName: "childReport", targetPath: "resultBucket.report" },
      ];

      // Act - Import phase with sourcePath
      childManager.importVariables(parentManager, inputMappings);

      // Simulate child execution
      expect(childManager.getVariable("childName")).toBe("Alice");

      // Set child output
      childManager.registerVariable({
        name: "childReport",
        type: "string",
        value: "Report for Alice",
        readonly: false,
      });

      // Export phase with targetPath
      childManager.exportVariables(parentManager, outputMappings);

      // Assert
      const resultBucket = parentManager.getVariable("resultBucket") as Record<string, unknown>;
      expect(resultBucket.report).toBe("Report for Alice");
    });
  });
});
