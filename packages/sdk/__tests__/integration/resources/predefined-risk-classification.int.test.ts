/**
 * Integration Test: Predefined Tool Risk Classification
 *
 * Tests TOOL_RISK_CLASSIFICATION, SECURITY_PRESETS, and all query functions.
 *
 * Real business scenarios:
 * 1. Auto-approval: Check a tool's risk level before deciding auto-approval policy
 * 2. Security presets: Apply SAFE/BALANCED/PERMISSIVE presets during SDK configuration
 * 3. Risk reporting: Get statistics on how many tools fall into each risk category
 * 4. Audit: Verify all predefined tools have a defined risk level
 */

import { describe, it, expect } from "vitest";
import {
  TOOL_RISK_CLASSIFICATION,
  SECURITY_PRESETS,
  getToolRiskLevel,
  hasKnownRiskLevel,
  getToolsByRiskLevel,
  getRiskLevelStats,
} from "@/resources/predefined/tools/risk-classification.js";
import type { ToolRiskLevel } from "@wf-agent/types";
import type { SecurityPreset } from "@wf-agent/types";

// =============================================================================
// Constants
// =============================================================================

const ALL_CLASSIFIED_TOOLS = Object.keys(TOOL_RISK_CLASSIFICATION);
const ALL_CLASSIFIED_COUNT = ALL_CLASSIFIED_TOOLS.length;

// Expected risk level distribution (derived from the classification map)
const EXPECTED_RISK_LEVELS: ToolRiskLevel[] = [
  "READ_ONLY", "WRITE", "EXECUTE", "MCP", "NETWORK", "SYSTEM", "INTERACTION",
];

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Tool Risk Classification", () => {
  // ---------------------------------------------------------------------------
  // Scenario: All classified tools have valid risk levels
  // ---------------------------------------------------------------------------
  describe("TOOL_RISK_CLASSIFICATION completeness", () => {
    it("should have all entries with valid risk levels", () => {
      for (const [toolId, level] of Object.entries(TOOL_RISK_CLASSIFICATION)) {
        expect(EXPECTED_RISK_LEVELS).toContain(level);
        expect(toolId).toBeTruthy();
        expect(typeof toolId).toBe("string");
      }
    });

    it("should classify read-only filesystem tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["read_file"]).toBe("READ_ONLY");
      expect(TOOL_RISK_CLASSIFICATION["list_files"]).toBe("READ_ONLY");
      expect(TOOL_RISK_CLASSIFICATION["grep"]).toBe("READ_ONLY");
    });

    it("should classify write filesystem tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["write_file"]).toBe("WRITE");
      expect(TOOL_RISK_CLASSIFICATION["edit"]).toBe("WRITE");
      expect(TOOL_RISK_CLASSIFICATION["apply_diff"]).toBe("WRITE");
      expect(TOOL_RISK_CLASSIFICATION["apply_patch"]).toBe("WRITE");
    });

    it("should classify shell tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["run_shell"]).toBe("EXECUTE");
      expect(TOOL_RISK_CLASSIFICATION["backend_shell"]).toBe("EXECUTE");
      expect(TOOL_RISK_CLASSIFICATION["shell_kill"]).toBe("EXECUTE");
      expect(TOOL_RISK_CLASSIFICATION["shell_output"]).toBe("READ_ONLY");
    });

    it("should classify interaction tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["ask_followup_question"]).toBe("INTERACTION");
      expect(TOOL_RISK_CLASSIFICATION["attempt_completion"]).toBe("INTERACTION");
    });

    it("should classify system tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["execute_workflow"]).toBe("SYSTEM");
      expect(TOOL_RISK_CLASSIFICATION["cancel_workflow"]).toBe("SYSTEM");
      expect(TOOL_RISK_CLASSIFICATION["call_agent"]).toBe("SYSTEM");
    });

    it("should classify MCP tools correctly", () => {
      expect(TOOL_RISK_CLASSIFICATION["use_mcp"]).toBe("MCP");
    });

    it("should have builtin prefixed aliases for builtin tools", () => {
      expect(TOOL_RISK_CLASSIFICATION["builtin_execute_workflow"]).toBe("SYSTEM");
      expect(TOOL_RISK_CLASSIFICATION["builtin_cancel_workflow"]).toBe("SYSTEM");
      expect(TOOL_RISK_CLASSIFICATION["builtin_query_workflow_status"]).toBe("READ_ONLY");
      expect(TOOL_RISK_CLASSIFICATION["builtin_call_agent"]).toBe("SYSTEM");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: getToolRiskLevel — query tool risk
  // ---------------------------------------------------------------------------
  describe("getToolRiskLevel", () => {
    it("should return the correct risk level for a known tool", () => {
      expect(getToolRiskLevel("read_file")).toBe("READ_ONLY");
      expect(getToolRiskLevel("run_shell")).toBe("EXECUTE");
      expect(getToolRiskLevel("use_mcp")).toBe("MCP");
    });

    it("should return WRITE as default for unknown tools", () => {
      expect(getToolRiskLevel("unknown_tool")).toBe("WRITE");
      expect(getToolRiskLevel("")).toBe("WRITE");
    });

    it("should return correct level for builtin prefixed tool IDs", () => {
      expect(getToolRiskLevel("builtin_execute_workflow")).toBe("SYSTEM");
      expect(getToolRiskLevel("builtin_call_agent")).toBe("SYSTEM");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: hasKnownRiskLevel — check if tool has defined risk
  // ---------------------------------------------------------------------------
  describe("hasKnownRiskLevel", () => {
    it("should return true for a tool in the classification", () => {
      expect(hasKnownRiskLevel("read_file")).toBe(true);
      expect(hasKnownRiskLevel("run_shell")).toBe(true);
    });

    it("should return false for an unknown tool", () => {
      expect(hasKnownRiskLevel("unknown_tool")).toBe(false);
      expect(hasKnownRiskLevel("")).toBe(false);
    });

    it("should return true for builtin prefixed tool IDs", () => {
      expect(hasKnownRiskLevel("builtin_execute_workflow")).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: getToolsByRiskLevel — filter by risk level
  // ---------------------------------------------------------------------------
  describe("getToolsByRiskLevel", () => {
    it("should return all READ_ONLY tools", () => {
      const readOnlyTools = getToolsByRiskLevel("READ_ONLY");

      expect(readOnlyTools.length).toBeGreaterThan(0);
      expect(readOnlyTools).toContain("read_file");
      expect(readOnlyTools).toContain("list_files");
      expect(readOnlyTools).toContain("grep");
      expect(readOnlyTools).toContain("recall_notes");
      expect(readOnlyTools).toContain("shell_output");
      expect(readOnlyTools).toContain("query_workflow_status");
    });

    it("should return all SYSTEM tools", () => {
      const systemTools = getToolsByRiskLevel("SYSTEM");

      expect(systemTools).toContain("execute_workflow");
      expect(systemTools).toContain("cancel_workflow");
      expect(systemTools).toContain("call_agent");
    });

    it("should return an empty array for NETWORK level (no tools classified)", () => {
      const networkTools = getToolsByRiskLevel("NETWORK");
      expect(networkTools).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: getRiskLevelStats — distribution statistics
  // ---------------------------------------------------------------------------
  describe("getRiskLevelStats", () => {
    it("should return stats for all risk levels", () => {
      const stats = getRiskLevelStats();

      for (const level of EXPECTED_RISK_LEVELS) {
        expect(stats).toHaveProperty(level);
        expect(typeof stats[level]).toBe("number");
        expect(stats[level]).toBeGreaterThanOrEqual(0);
      }
    });

    it("should have the sum of all stats equal to total classified tools", () => {
      const stats = getRiskLevelStats();
      const total = Object.values(stats).reduce((sum, count) => sum + count, 0);

      expect(total).toBe(ALL_CLASSIFIED_COUNT);
    });

    it("should have more than zero tools in READ_ONLY, WRITE, and EXECUTE levels", () => {
      const stats = getRiskLevelStats();

      expect(stats.READ_ONLY).toBeGreaterThan(0);
      expect(stats.WRITE).toBeGreaterThan(0);
      expect(stats.EXECUTE).toBeGreaterThan(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: SECURITY_PRESETS — verify presets completeness
  // ---------------------------------------------------------------------------
  describe("SECURITY_PRESETS", () => {
    const presetNames: SecurityPreset[] = ["SAFE", "BALANCED", "PERMISSIVE"];

    it("should define all three presets", () => {
      for (const name of presetNames) {
        expect(SECURITY_PRESETS[name]).toBeDefined();
      }
    });

    it("should have categories defined for each preset", () => {
      for (const name of presetNames) {
        const preset = SECURITY_PRESETS[name]!;
        expect(preset.categories).toBeDefined();
        expect(preset.categories!.alwaysAllowReadOnly).toBeDefined();
        expect(preset.categories!.alwaysAllowWrite).toBeDefined();
        expect(preset.categories!.alwaysAllowExecute).toBeDefined();
      }
    });

    it("should have workspace boundary settings for each preset", () => {
      for (const name of presetNames) {
        const preset = SECURITY_PRESETS[name]!;
        expect(preset.workspaceBoundary).toBeDefined();
        expect(preset.workspaceBoundary!.allowReadOnlyOutsideWorkspace).toBeDefined();
        expect(preset.workspaceBoundary!.allowWriteOutsideWorkspace).toBeDefined();
      }
    });

    it("should have allowWriteProtected defined for each preset", () => {
      for (const name of presetNames) {
        expect(SECURITY_PRESETS[name]!.allowWriteProtected).toBeDefined();
      }
    });

    it("SAFE preset should only allow read-only by default", () => {
      const safe = SECURITY_PRESETS.SAFE!;
      expect(safe.categories!.alwaysAllowReadOnly).toBe(true);
      expect(safe.categories!.alwaysAllowWrite).toBe(false);
      expect(safe.categories!.alwaysAllowExecute).toBe(false);
      expect(safe.categories!.alwaysAllowMcp).toBe(false);
      expect(safe.categories!.alwaysAllowNetwork).toBe(false);
      expect(safe.categories!.alwaysAllowInteraction).toBe(false);
      expect(safe.workspaceBoundary!.allowReadOnlyOutsideWorkspace).toBe(false);
      expect(safe.workspaceBoundary!.allowWriteOutsideWorkspace).toBe(false);
      expect(safe.allowWriteProtected).toBe(false);
    });

    it("BALANCED preset should allow read-only, write, and interaction", () => {
      const balanced = SECURITY_PRESETS.BALANCED!;
      expect(balanced.categories!.alwaysAllowReadOnly).toBe(true);
      expect(balanced.categories!.alwaysAllowWrite).toBe(true);
      expect(balanced.categories!.alwaysAllowExecute).toBe(false);
      expect(balanced.categories!.alwaysAllowMcp).toBe(false);
      expect(balanced.categories!.alwaysAllowNetwork).toBe(false);
      expect(balanced.categories!.alwaysAllowInteraction).toBe(true);
      expect(balanced.workspaceBoundary!.allowReadOnlyOutsideWorkspace).toBe(true);
      expect(balanced.workspaceBoundary!.allowWriteOutsideWorkspace).toBe(false);
      expect(balanced.allowWriteProtected).toBe(false);
    });

    it("PERMISSIVE preset should allow everything", () => {
      const permissive = SECURITY_PRESETS.PERMISSIVE!;
      expect(permissive.categories!.alwaysAllowReadOnly).toBe(true);
      expect(permissive.categories!.alwaysAllowWrite).toBe(true);
      expect(permissive.categories!.alwaysAllowExecute).toBe(true);
      expect(permissive.categories!.alwaysAllowMcp).toBe(true);
      expect(permissive.categories!.alwaysAllowNetwork).toBe(true);
      expect(permissive.categories!.alwaysAllowInteraction).toBe(true);
      expect(permissive.workspaceBoundary!.allowReadOnlyOutsideWorkspace).toBe(true);
      expect(permissive.workspaceBoundary!.allowWriteOutsideWorkspace).toBe(true);
      expect(permissive.allowWriteProtected).toBe(true);
    });
  });
});