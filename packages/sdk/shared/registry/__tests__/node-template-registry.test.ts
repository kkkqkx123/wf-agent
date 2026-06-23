import { describe, it, expect, beforeEach } from "vitest";
import { NodeTemplateRegistry } from "../node-template-registry.js";
import type { NodeTemplate } from "@wf-agent/types";

function createValidStartTemplate(overrides: Partial<NodeTemplate> = {}): NodeTemplate {
  return {
    name: "test-start",
    type: "START",
    config: {},
    description: "A test start node",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    metadata: { category: "control", tags: ["start"] },
    ...overrides,
  } as NodeTemplate;
}

function createValidLLMTemplate(overrides: Partial<NodeTemplate> = {}): NodeTemplate {
  return {
    name: "test-llm",
    type: "LLM",
    config: { profileId: "gpt-4o" },
    description: "A test LLM node",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  } as NodeTemplate;
}

describe("NodeTemplateRegistry", () => {
  let registry: NodeTemplateRegistry;

  beforeEach(() => {
    registry = new NodeTemplateRegistry();
  });

  describe("register", () => {
    it("should register a valid node template", () => {
      const template = createValidStartTemplate();
      registry.register(template);
      expect(registry.has(template.name)).toBe(true);
    });

    it("should throw if name already exists", () => {
      registry.register(createValidStartTemplate());
      expect(() => registry.register(createValidStartTemplate())).toThrow("already exists");
    });

    it("should throw if name is empty", () => {
      expect(() => registry.register(createValidStartTemplate({ name: "" }))).toThrow(
        "is required",
      );
    });

    it("should throw if type is invalid", () => {
      expect(() => registry.register(createValidStartTemplate({ type: "INVALID" as any }))).toThrow(
        "Invalid node type",
      );
    });

    it("should throw if config is missing", () => {
      expect(() =>
        registry.register(createValidStartTemplate({ config: undefined as any })),
      ).toThrow("config is required");
    });

    it("should throw if node configuration validation fails", () => {
      expect(() => registry.register(createValidLLMTemplate({ config: {} }))).toThrow(
        "Invalid node configuration",
      );
    });
  });

  describe("registerBatch", () => {
    it("should register multiple templates", () => {
      const t1 = createValidStartTemplate({ name: "t1" });
      const t2 = createValidStartTemplate({ name: "t2" });
      registry.registerBatch([t1, t2]);
      expect(registry.size).toBe(2);
    });

    it("should throw on first invalid template", () => {
      expect(() =>
        registry.registerBatch([
          createValidStartTemplate({ name: "t1" }),
          createValidStartTemplate({ name: "" }),
        ]),
      ).toThrow();
    });
  });

  describe("get", () => {
    it("should return template by name", () => {
      const template = createValidStartTemplate();
      registry.register(template);
      expect(registry.get(template.name)).toEqual(template);
    });

    it("should return undefined for non-existent template", () => {
      expect(registry.get("non-existent")).toBeUndefined();
    });
  });

  describe("has", () => {
    it("should return true if template exists", () => {
      registry.register(createValidStartTemplate());
      expect(registry.has("test-start")).toBe(true);
    });

    it("should return false if template does not exist", () => {
      expect(registry.has("non-existent")).toBe(false);
    });
  });

  describe("update", () => {
    it("should update an existing template", () => {
      registry.register(createValidStartTemplate());
      registry.update("test-start", { description: "Updated description" });
      expect(registry.get("test-start")?.description).toBe("Updated description");
    });

    it("should update updatedAt timestamp", () => {
      registry.register(createValidStartTemplate());
      const before = registry.get("test-start")!.updatedAt;
      registry.update("test-start", { description: "Updated" });
      expect(registry.get("test-start")!.updatedAt).toBeGreaterThanOrEqual(before);
    });

    it("should throw if template does not exist", () => {
      expect(() => registry.update("non-existent", { description: "test" })).toThrow("not found");
    });

    it("should throw if updated config is invalid", () => {
      registry.register(createValidLLMTemplate());
      expect(() => registry.update("test-llm", { config: {} })).toThrow(
        "Invalid node configuration",
      );
    });
  });

  describe("unregister", () => {
    it("should delete a template", () => {
      registry.register(createValidStartTemplate());
      registry.unregister("test-start");
      expect(registry.has("test-start")).toBe(false);
    });

    it("should throw if template does not exist", () => {
      expect(() => registry.unregister("non-existent")).toThrow("not found");
    });
  });

  describe("unregisterBatch", () => {
    it("should delete multiple templates", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      registry.register(createValidStartTemplate({ name: "t2" }));
      registry.unregisterBatch(["t1", "t2"]);
      expect(registry.size).toBe(0);
    });

    it("should throw on first non-existent template", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      expect(() => registry.unregisterBatch(["t1", "non-existent"])).toThrow("not found");
    });
  });

  describe("list", () => {
    it("should return all templates", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      registry.register(createValidStartTemplate({ name: "t2" }));
      expect(registry.list()).toHaveLength(2);
    });

    it("should return empty array when no templates", () => {
      expect(registry.list()).toEqual([]);
    });
  });

  describe("listSummaries", () => {
    it("should return summaries with metadata fields", () => {
      registry.register(
        createValidStartTemplate({
          name: "t1",
          metadata: { category: "control", tags: ["start"] },
        }),
      );
      const summaries = registry.listSummaries();
      expect(summaries).toHaveLength(1);
      expect(summaries[0]!.name).toBe("t1");
      expect(summaries[0]!.category).toBe("control");
      expect(summaries[0]!.tags).toEqual(["start"]);
    });

    it("should return summaries without optional metadata", () => {
      registry.register(createValidStartTemplate({ name: "t1", metadata: undefined }));
      const summaries = registry.listSummaries();
      expect(summaries[0]!.category).toBeUndefined();
      expect(summaries[0]!.tags).toBeUndefined();
    });
  });

  describe("listByType", () => {
    it("should filter by node type", () => {
      registry.register(createValidStartTemplate({ name: "start-1" }));
      registry.register(createValidLLMTemplate({ name: "llm-1" }));
      const startTemplates = registry.listByType("START");
      expect(startTemplates).toHaveLength(1);
      expect(startTemplates[0]!.name).toBe("start-1");
    });
  });

  describe("listByCategory", () => {
    it("should filter by category", () => {
      registry.register(
        createValidStartTemplate({ name: "t1", metadata: { category: "control" } }),
      );
      registry.register(createValidStartTemplate({ name: "t2", metadata: { category: "data" } }));
      expect(registry.listByCategory("control")).toHaveLength(1);
    });
  });

  describe("listByTags", () => {
    it("should filter by tags - all must match", () => {
      registry.register(
        createValidStartTemplate({ name: "t1", metadata: { tags: ["start", "basic"] } }),
      );
      registry.register(createValidStartTemplate({ name: "t2", metadata: { tags: ["start"] } }));
      expect(registry.listByTags(["start", "basic"])).toHaveLength(1);
    });

    it("should handle missing tags gracefully", () => {
      registry.register(createValidStartTemplate({ name: "t1", metadata: undefined }));
      expect(registry.listByTags(["start"])).toHaveLength(0);
    });
  });

  describe("clear", () => {
    it("should remove all templates", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      registry.register(createValidStartTemplate({ name: "t2" }));
      registry.clear();
      expect(registry.size).toBe(0);
    });
  });

  describe("size", () => {
    it("should return 0 for empty registry", () => {
      expect(registry.size).toBe(0);
    });

    it("should return correct count", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      expect(registry.size).toBe(1);
    });
  });

  describe("search", () => {
    it("should search by name", () => {
      registry.register(createValidStartTemplate({ name: "my-start-node" }));
      expect(registry.search("start")).toHaveLength(1);
    });

    it("should search by description", () => {
      registry.register(
        createValidStartTemplate({ name: "t1", description: "custom description" }),
      );
      expect(registry.search("custom")).toHaveLength(1);
    });

    it("should search by tags", () => {
      registry.register(
        createValidStartTemplate({ name: "t1", metadata: { tags: ["important", "urgent"] } }),
      );
      expect(registry.search("important")).toHaveLength(1);
    });

    it("should search by category", () => {
      registry.register(
        createValidStartTemplate({ name: "t1", metadata: { category: "machine-learning" } }),
      );
      expect(registry.search("machine")).toHaveLength(1);
    });

    it("should return empty array for no match", () => {
      registry.register(createValidStartTemplate({ name: "t1" }));
      expect(registry.search("nonexistent")).toHaveLength(0);
    });

    it("should be case insensitive", () => {
      registry.register(createValidStartTemplate({ name: "MyNode" }));
      expect(registry.search("mynode")).toHaveLength(1);
    });
  });

  describe("export / import", () => {
    it("should export template as JSON string", () => {
      registry.register(createValidStartTemplate());
      const json = registry.export("test-start");
      const parsed = JSON.parse(json);
      expect(parsed.name).toBe("test-start");
      expect(parsed.type).toBe("START");
    });

    it("should throw on export of non-existent template", () => {
      expect(() => registry.export("non-existent")).toThrow("not found");
    });

    it("should import a valid template from JSON", () => {
      const template = createValidStartTemplate({ name: "imported-template" });
      const json = JSON.stringify(template);
      const name = registry.import(json);
      expect(name).toBe("imported-template");
      expect(registry.has("imported-template")).toBe(true);
    });

    it("should throw on import of invalid JSON", () => {
      expect(() => registry.import("not json")).toThrow("Failed to import");
    });

    it("should throw on import with duplicate name", () => {
      registry.register(createValidStartTemplate({ name: "dup" }));
      const json = JSON.stringify(createValidStartTemplate({ name: "dup" }));
      expect(() => registry.import(json)).toThrow("already exists");
    });
  });
});
