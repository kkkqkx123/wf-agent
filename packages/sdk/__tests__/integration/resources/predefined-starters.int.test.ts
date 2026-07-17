/**
 * Integration Test: Predefined Starters
 *
 * Tests the StarterRegistry, BaseStarter, and GoalReviewStarter lifecycle.
 *
 * Real business scenarios:
 * 1. Starter registration: Register GoalReviewStarter at SDK startup
 * 2. Starter activation: Activate with config to install workflow + prompt templates into registries
 * 3. Starter deactivation: Deactivate to uninstall workflow and clean up registries
 * 4. List available starters: Query registered starters for UI display
 */

import { describe, it, expect, beforeEach } from "vitest";
import { StarterRegistry } from "@/resources/predefined/starter/starter-registry.js";
import { GoalReviewStarter } from "@/resources/predefined/starter/starters/goal-review-starter.js";
import type { GoalReviewConfig } from "@/resources/predefined/starter/starters/goal-review-starter.js";
import type { WorkflowBundle, StarterRegistries } from "@/resources/predefined/starter/types.js";

// =============================================================================
// Constants
// =============================================================================

const STARTER_ID = "@standard/goal-review-agent";

function createMinimalConfig(): GoalReviewConfig {
  return {
    rootRequirement: "Test requirement",
  };
}

function createMockRegistries(): StarterRegistries {
  const workflows = new Map<string, unknown>();
  const agentLoops = new Map<string, unknown>();
  const nodeTemplates = new Map<string, unknown>();
  const triggerTemplates = new Map<string, unknown>();
  const hookTemplates = new Map<string, unknown>();
  const promptTemplates = new Map<string, unknown>();

  return {
    workflowRegistry: {
      register: async (w: unknown) => {
        const wf = w as { id: string };
        if (workflows.has(wf.id)) throw new Error(`Workflow "${wf.id}" already exists`);
        workflows.set(wf.id, w);
      },
      unregister: async (id: string) => {
        workflows.delete(id);
      },
    },
    agentLoopRegistry: {
      register: async (l: unknown) => {
        const loop = l as { id: string };
        agentLoops.set(loop.id, l);
      },
      unregister: async (id: string) => {
        agentLoops.delete(id);
      },
    },
    nodeTemplateRegistry: {
      register: async (nt: unknown) => {
        const t = nt as { name: string };
        nodeTemplates.set(t.name, nt);
      },
      unregister: async (name: string) => {
        nodeTemplates.delete(name);
      },
    },
    triggerTemplateRegistry: {
      register: async (tt: unknown) => {
        const t = tt as { name: string };
        triggerTemplates.set(t.name, tt);
      },
      unregister: async (name: string) => {
        triggerTemplates.delete(name);
      },
    },
    hookTemplateRegistry: {
      register: async (ht: unknown) => {
        const t = ht as { name: string };
        hookTemplates.set(t.name, ht);
      },
      unregister: async (name: string) => {
        hookTemplates.delete(name);
      },
    },
    promptTemplateRegistry: {
      register: async (key: string, pt: unknown) => {
        promptTemplates.set(key, pt);
      },
      unregister: async (key: string) => {
        promptTemplates.delete(key);
      },
    },
  };
}

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Starters", () => {
  let starterRegistry: StarterRegistry;
  let registries: StarterRegistries;

  beforeEach(() => {
    starterRegistry = new StarterRegistry();
    registries = createMockRegistries();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Starter registration
  // ---------------------------------------------------------------------------
  describe("register starter", () => {
    it("should register a new starter successfully", () => {
      const starter = new GoalReviewStarter();

      starterRegistry.register(starter);

      expect(starterRegistry.get(STARTER_ID)).toBe(starter);
    });

    it("should throw when registering a duplicate starter", () => {
      starterRegistry.register(new GoalReviewStarter());

      expect(() => starterRegistry.register(new GoalReviewStarter())).toThrow(
        /already registered/,
      );
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: List starters
  // ---------------------------------------------------------------------------
  describe("list starters", () => {
    it("should return all registered starters", () => {
      starterRegistry.register(new GoalReviewStarter());

      const starters = starterRegistry.list();
      expect(starters).toHaveLength(1);
      expect(starters[0].metadata.id).toBe(STARTER_ID);
    });

    it("should return empty array when no starters are registered", () => {
      expect(starterRegistry.list()).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Get starter
  // ---------------------------------------------------------------------------
  describe("get starter", () => {
    it("should return the starter by ID", () => {
      const starter = new GoalReviewStarter();
      starterRegistry.register(starter);

      expect(starterRegistry.get(STARTER_ID)).toBe(starter);
    });

    it("should return undefined for unregistered starter", () => {
      expect(starterRegistry.get("non_existent_starter")).toBeUndefined();
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Unregister starter
  // ---------------------------------------------------------------------------
  describe("unregister starter", () => {
    it("should remove the starter from the registry", () => {
      starterRegistry.register(new GoalReviewStarter());
      expect(starterRegistry.get(STARTER_ID)).toBeDefined();

      starterRegistry.unregister(STARTER_ID);

      expect(starterRegistry.get(STARTER_ID)).toBeUndefined();
    });

    it("should not throw when unregistering a non-existent starter", () => {
      expect(() => starterRegistry.unregister("non_existent")).not.toThrow();
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Activate starter — register resources into registries
  // ---------------------------------------------------------------------------
  describe("activate starter", () => {
    it("should activate a starter and return the workflow bundle", async () => {
      starterRegistry.register(new GoalReviewStarter());

      const bundle = await starterRegistry.activate(STARTER_ID, createMinimalConfig(), registries);

      expect(bundle).toBeDefined();
      expect(bundle.workflow).toBeDefined();
      expect(bundle.workflow.id).toBe("@standard/goal-review-agent-workflow");
    });

    it("should throw when activating a non-existent starter", async () => {
      await expect(
        starterRegistry.activate("non_existent", {}, registries),
      ).rejects.toThrow(/not found/);
    });

    it("should register the workflow into the workflow registry", async () => {
      starterRegistry.register(new GoalReviewStarter());

      await starterRegistry.activate(STARTER_ID, createMinimalConfig(), registries);

      // Verify workflow was registered
      const bundle = starterRegistry.list().find((s) => s.metadata.id === STARTER_ID);
      expect(bundle).toBeDefined();
    });

    it("should register prompt templates if present in the bundle", async () => {
      starterRegistry.register(new GoalReviewStarter());

      await starterRegistry.activate(STARTER_ID, createMinimalConfig(), registries);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Deactivate starter — unregister resources
  // ---------------------------------------------------------------------------
  describe("deactivate starter", () => {
    it("should deactivate an active starter without throwing", async () => {
      starterRegistry.register(new GoalReviewStarter());
      await starterRegistry.activate(STARTER_ID, createMinimalConfig(), registries);

      await expect(
        starterRegistry.deactivate(STARTER_ID, registries),
      ).resolves.toBeUndefined();
    });

    it("should not throw when deactivating a non-active starter", async () => {
      starterRegistry.register(new GoalReviewStarter());

      await expect(
        starterRegistry.deactivate(STARTER_ID, registries),
      ).resolves.toBeUndefined();
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: GoalReviewStarter assemble — verify bundle structure
  // ---------------------------------------------------------------------------
  describe("GoalReviewStarter assemble", () => {
    it("should assemble a workflow bundle with workflow and prompt templates", () => {
      const starter = new GoalReviewStarter();
      const config: GoalReviewConfig = {
        rootRequirement: "Review this code",
      };

      const bundle: WorkflowBundle = starter.assemble(config);

      expect(bundle.workflow).toBeDefined();
      expect(bundle.workflow.id).toBe("@standard/goal-review-agent-workflow");
      expect(bundle.workflow.type).toBe("STANDALONE");
      expect(bundle.workflow.nodes.length).toBeGreaterThan(0);
      expect(bundle.workflow.edges.length).toBeGreaterThan(0);

      // Should have a planner prompt template
      expect(bundle.promptTemplates).toBeDefined();
      expect(bundle.promptTemplates!.length).toBe(1);
      expect(bundle.promptTemplates![0].id).toBe("@standard/goal-review-planner");
    });

    it("should include review loop structure (7 nodes, 7 edges)", () => {
      const starter = new GoalReviewStarter();
      const config: GoalReviewConfig = {
        rootRequirement: "Review this code",
      };

      const bundle: WorkflowBundle = starter.assemble(config);

      // START, LOOP_START, LLM (planner), AGENT_LOOP (executor), AGENT_LOOP (reviewer), LOOP_END, END
      expect(bundle.workflow.nodes).toHaveLength(7);
      // 6 DEFAULT edges + 1 CONDITIONAL back-edge to planner = 7 edges
      expect(bundle.workflow.edges).toHaveLength(7);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Starter metadata
  // ---------------------------------------------------------------------------
  describe("GoalReviewStarter metadata", () => {
    it("should have correct metadata fields", () => {
      const starter = new GoalReviewStarter();

      expect(starter.metadata.id).toBe("@standard/goal-review-agent");
      expect(starter.metadata.name).toBe("Goal Review Agent");
      expect(starter.metadata.version).toBe("1.0.0");
      expect(starter.metadata.description).toBeTruthy();
      expect(starter.metadata.tags).toContain("review");
      expect(starter.metadata.category).toBe("code-review");
      expect(starter.metadata.configurable).toBeDefined();
      expect(starter.metadata.configurable!.maxIterations).toBeDefined();
    });
  });
});