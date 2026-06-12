/**
 * Unit Tests for Execution Hierarchy Builder
 *
 * Tests the setupHierarchy, teardownHierarchy, and validateHierarchy functions.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { setupHierarchy, teardownHierarchy, validateHierarchy } from "../hierarchy-builder.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { ExecutionHierarchyRegistry } from "../../../../core/registry/execution-hierarchy-registry.js";
import type { ID, ParentExecutionContext, ExecutionHierarchyMetadata } from "@wf-agent/types";

// ============================================================================
// Mock Factory Helpers
// ============================================================================

function createMockRegistry(
  methods?: Partial<ExecutionHierarchyRegistry>,
): ExecutionHierarchyRegistry {
  return {
    register: vi.fn(),
    unregister: vi.fn(),
    get: vi.fn(),
    has: vi.fn(),
    getAll: vi.fn().mockReturnValue([]),
    getAllIds: vi.fn().mockReturnValue([]),
    size: vi.fn().mockReturnValue(0),
    clear: vi.fn(),
    getAllDescendants: vi.fn().mockReturnValue([]),
    getDirectChildren: vi.fn().mockReturnValue([]),
    cleanupHierarchy: vi.fn(),
    getHierarchy: vi.fn(),
    getExecutionsByRoot: vi.fn(),
    ...methods,
  } as unknown as ExecutionHierarchyRegistry;
}

interface MockEntityOptions {
  id?: ID;
  parentContext?: ParentExecutionContext | undefined;
  childExecutionIds?: ID[];
  hierarchyMetadata?: ExecutionHierarchyMetadata | undefined;
  /** If true, the mock entity will expose a hierarchyManager with a registry reference */
  registry?: ExecutionHierarchyRegistry | undefined;
}

function createMockEntity(options: MockEntityOptions = {}): WorkflowExecutionEntity {
  const {
    id = "test-entity-id",
    parentContext = undefined,
    childExecutionIds = [],
    hierarchyMetadata = undefined,
    registry = undefined,
  } = options;

  const setParentContext = vi.fn();
  const registerChild = vi.fn();
  const unregisterChild = vi.fn();
  const getParentContext = vi.fn().mockReturnValue(parentContext);
  const getChildExecutionIds = vi.fn().mockReturnValue(childExecutionIds);
  const getHierarchyMetadata = vi.fn().mockReturnValue(hierarchyMetadata);

  const entity = {
    id,
    setParentContext,
    registerChild,
    unregisterChild,
    getParentContext,
    getChildExecutionIds,
    getHierarchyMetadata,
  } as unknown as WorkflowExecutionEntity;

  // If registry is provided, attach it to mimic the private hierarchyManager
  if (registry) {
    (
      entity as unknown as { hierarchyManager?: { registry?: ExecutionHierarchyRegistry } }
    ).hierarchyManager = {
      registry,
    };
  }

  return entity;
}

// ============================================================================
// Tests
// ============================================================================

describe("setupHierarchy", () => {
  let parentEntity: WorkflowExecutionEntity;
  let childEntity: WorkflowExecutionEntity;
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = createMockRegistry();
    parentEntity = createMockEntity({ id: "parent-1", registry });
    childEntity = createMockEntity({ id: "child-1", registry });
  });

  it("should set parent context, register child, and add child reference", async () => {
    await setupHierarchy({ parentEntity, childEntity });

    // Verify parent context is set on child
    expect(childEntity.setParentContext).toHaveBeenCalledTimes(1);
    const parentContextArg = (childEntity.setParentContext as ReturnType<typeof vi.fn>).mock
      .calls[0]![0];
    expect(parentContextArg).toMatchObject({
      parentType: "WORKFLOW",
      parentId: "parent-1",
    });

    // Verify child is registered in hierarchy registry
    expect(registry.register).toHaveBeenCalledTimes(1);
    expect(registry.register).toHaveBeenCalledWith(childEntity);

    // Verify child reference is registered in parent
    expect(parentEntity.registerChild).toHaveBeenCalledTimes(1);
    const childRefArg = (parentEntity.registerChild as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(childRefArg).toMatchObject({
      childType: "WORKFLOW",
      childId: "child-1",
    });
    expect(typeof childRefArg.createdAt).toBe("number");
  });

  it("should include nodeId when provided", async () => {
    const nodeId = "subgraph-node-123";
    await setupHierarchy({ parentEntity, childEntity, nodeId });

    const parentContextArg = (childEntity.setParentContext as ReturnType<typeof vi.fn>).mock
      .calls[0]![0];
    expect(parentContextArg).toMatchObject({
      parentType: "WORKFLOW",
      parentId: "parent-1",
      nodeId,
    });
  });

  it("should include custom childMetadata when provided", async () => {
    const childMetadata = { forkPathId: "path-1", createdAt: Date.now() };
    await setupHierarchy({ parentEntity, childEntity, childMetadata });

    const childRefArg = (parentEntity.registerChild as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(childRefArg).toMatchObject({
      childType: "WORKFLOW",
      childId: "child-1",
      forkPathId: "path-1",
    });
  });

  it("should handle missing hierarchy registry gracefully", async () => {
    // Create child entity without registry reference
    const childWithoutRegistry = createMockEntity({ id: "child-no-reg" });
    const registryRegisterSpy = vi.spyOn(registry, "register");

    await setupHierarchy({ parentEntity, childEntity: childWithoutRegistry });

    // Parent context should still be set
    expect(childWithoutRegistry.setParentContext).toHaveBeenCalledTimes(1);

    // Registry register should not be called (no registry available on child entity)
    expect(registryRegisterSpy).not.toHaveBeenCalled();

    // Parent should NOT register child reference since function returns early
    expect(parentEntity.registerChild).not.toHaveBeenCalled();
  });
});

describe("teardownHierarchy", () => {
  let parentEntity: WorkflowExecutionEntity;
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = createMockRegistry();
    parentEntity = createMockEntity({ id: "parent-1" });
  });

  it("should unregister child from parent (registry unregister is a placeholder)", async () => {
    // Note: getHierarchyRegistryById is currently a placeholder that always returns undefined,
    // so registry.unregister is not expected to be called in the current implementation.
    const childEntity = createMockEntity({ id: "child-1", registry });

    await teardownHierarchy(parentEntity, childEntity.id);

    // Verify child reference is removed from parent
    expect(parentEntity.unregisterChild).toHaveBeenCalledTimes(1);
    expect(parentEntity.unregisterChild).toHaveBeenCalledWith("child-1", "WORKFLOW");

    // Registry unregister is not called because getHierarchyRegistryById is a placeholder
    expect(registry.unregister).not.toHaveBeenCalled();
  });

  it("should skip registry unregister when unregisterFromRegistry is false", async () => {
    const childEntity = createMockEntity({ id: "child-1", registry });

    await teardownHierarchy(parentEntity, childEntity.id, false);

    // Verify child reference is removed from parent
    expect(parentEntity.unregisterChild).toHaveBeenCalledTimes(1);

    // Verify registry unregister is NOT called
    expect(registry.unregister).not.toHaveBeenCalled();
  });

  it("should handle missing registry gracefully", async () => {
    // child entity without registry reference
    await teardownHierarchy(parentEntity, "child-no-reg");

    // Parent should still unregister child reference
    expect(parentEntity.unregisterChild).toHaveBeenCalledTimes(1);
    expect(parentEntity.unregisterChild).toHaveBeenCalledWith("child-no-reg", "WORKFLOW");
  });
});

describe("validateHierarchy", () => {
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = createMockRegistry();
  });

  it("should return valid for a root entity with no issues", () => {
    const entity = createMockEntity({
      id: "root-entity",
      parentContext: undefined,
      childExecutionIds: [],
      hierarchyMetadata: {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: "root-entity",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(true);
    expect(result.issues).toHaveLength(0);
  });

  it("should return valid for an entity with existing parent and children in registry", () => {
    const parentId = "parent-1";
    const childId = "child-1";

    // Mock registry.get to return entities for both parent and child
    const parentMock = createMockEntity({ id: parentId });
    const childMock = createMockEntity({ id: childId });
    vi.mocked(registry.get).mockImplementation((id: ID) => {
      if (id === parentId) return parentMock;
      if (id === childId) return childMock;
      return undefined;
    });

    const entity = createMockEntity({
      id: "entity-1",
      parentContext: { parentType: "WORKFLOW", parentId },
      childExecutionIds: [childId],
      hierarchyMetadata: {
        parent: { parentType: "WORKFLOW", parentId },
        children: [],
        depth: 1,
        rootExecutionId: parentId,
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(true);
    expect(result.issues).toHaveLength(0);
  });

  it("should detect missing parent in registry", () => {
    // Mock registry.get to return undefined for parent
    vi.mocked(registry.get).mockReturnValue(undefined);

    const entity = createMockEntity({
      id: "entity-1",
      parentContext: { parentType: "WORKFLOW", parentId: "missing-parent" },
      childExecutionIds: [],
      hierarchyMetadata: {
        parent: { parentType: "WORKFLOW", parentId: "missing-parent" },
        children: [],
        depth: 1,
        rootExecutionId: "missing-parent",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Parent execution missing-parent not found in registry");
  });

  it("should detect missing children in registry", () => {
    // Mock registry.get to return entity for parent but undefined for child
    const parentMock = createMockEntity({ id: "parent-1" });
    vi.mocked(registry.get).mockImplementation((id: ID) => {
      if (id === "parent-1") return parentMock;
      return undefined;
    });

    const entity = createMockEntity({
      id: "entity-1",
      parentContext: { parentType: "WORKFLOW", parentId: "parent-1" },
      childExecutionIds: ["missing-child"],
      hierarchyMetadata: {
        parent: { parentType: "WORKFLOW", parentId: "parent-1" },
        children: [],
        depth: 1,
        rootExecutionId: "parent-1",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Child execution missing-child not found in registry");
  });

  it("should detect invalid depth", () => {
    const entity = createMockEntity({
      id: "entity-1",
      parentContext: undefined,
      childExecutionIds: [],
      hierarchyMetadata: {
        parent: undefined,
        children: [],
        depth: -1,
        rootExecutionId: "entity-1",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Invalid depth: -1");
  });

  it("should detect missing root execution ID", () => {
    const entity = createMockEntity({
      id: "entity-1",
      parentContext: undefined,
      childExecutionIds: [],
      hierarchyMetadata: {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: "",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Root execution ID is missing");
  });

  it("should handle missing hierarchy metadata gracefully", () => {
    const entity = createMockEntity({
      id: "entity-1",
      parentContext: undefined,
      childExecutionIds: [],
      hierarchyMetadata: undefined,
      registry,
    });

    const result = validateHierarchy(entity);

    // Without metadata, only check for parent and children (both empty)
    expect(result.valid).toBe(true);
    expect(result.issues).toHaveLength(0);
  });

  it("should handle missing hierarchy registry gracefully", () => {
    // Entity without registry reference
    const entity = createMockEntity({
      id: "entity-1",
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Hierarchy registry not available");
  });

  it("should report multiple issues at once", () => {
    // Mock registry.get to return undefined for everything
    vi.mocked(registry.get).mockReturnValue(undefined);

    const entity = createMockEntity({
      id: "entity-1",
      parentContext: { parentType: "WORKFLOW", parentId: "missing-parent" },
      childExecutionIds: ["missing-child-1", "missing-child-2"],
      hierarchyMetadata: {
        parent: { parentType: "WORKFLOW", parentId: "missing-parent" },
        children: [],
        depth: -1,
        rootExecutionId: "",
        rootExecutionType: "WORKFLOW",
      },
      registry,
    });

    const result = validateHierarchy(entity);

    expect(result.valid).toBe(false);
    expect(result.issues).toContain("Parent execution missing-parent not found in registry");
    expect(result.issues).toContain("Child execution missing-child-1 not found in registry");
    expect(result.issues).toContain("Child execution missing-child-2 not found in registry");
    expect(result.issues).toContain("Invalid depth: -1");
    expect(result.issues).toContain("Root execution ID is missing");
  });
});
