/**
 * Protocol Consistency Validator Tests
 *
 * Tests for validateWorkflowToolCallProtocolConsistency covering:
 * - No relevant nodes (no LLM/AGENT_LOOP nodes)
 * - All nodes use same protocol (consistent)
 * - Nodes with different protocols (inconsistent)
 * - Node-profile compatibility check
 * - Edge cases (missing profile, missing config)
 */
import { describe, it, expect } from "vitest";
import { validateWorkflowToolCallProtocolConsistency } from "../protocol-consistency-validator.js";
import type { StaticNode, LLMProfile } from "@wf-agent/types";

describe("validateWorkflowToolCallProtocolConsistency", () => {
  const profileResolver = (id: string): LLMProfile | undefined => {
    const profiles: Record<string, LLMProfile> = {
      "profile-native": {
        id: "profile-native",
        model: "gpt-4",
        provider: "openai",
        toolCallFormat: { format: "native" },
      } as LLMProfile,
      "profile-xml": {
        id: "profile-xml",
        model: "claude-3",
        provider: "anthropic",
        toolCallFormat: { format: "xml" },
      } as LLMProfile,
      "profile-no-format": {
        id: "profile-no-format",
        model: "gpt-4",
        provider: "openai",
      } as LLMProfile,
    };
    return profiles[id];
  };

  // ===========================================================================
  // Scenario A: No relevant nodes — skip validation
  // ===========================================================================
  describe("no relevant nodes", () => {
    it("should return consistent=true when no nodes have profileId", () => {
      const nodes: StaticNode[] = [
        { id: "start-1", type: "START", config: {} } as StaticNode,
        { id: "end-1", type: "END", config: {} } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults).toHaveLength(0);
      expect(result.errors).toHaveLength(0);
    });

    it("should return consistent=true when nodes array is empty", () => {
      const result = validateWorkflowToolCallProtocolConsistency([], profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults).toHaveLength(0);
    });
  });

  // ===========================================================================
  // Scenario B: All nodes use same protocol — consistent
  // ===========================================================================
  describe("all nodes use same protocol", () => {
    it("should return consistent=true when all LLM nodes use the same profile", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: { profileId: "profile-native" },
        } as StaticNode,
        {
          id: "llm-2",
          type: "LLM",
          config: { profileId: "profile-native" },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults).toHaveLength(2);
    });

    it("should return consistent=true when all AGENT_LOOP nodes use the same protocol", () => {
      const nodes: StaticNode[] = [
        {
          id: "agent-1",
          type: "AGENT_LOOP",
          config: { inlineConfig: { profileId: "profile-xml" } },
        } as StaticNode,
        {
          id: "agent-2",
          type: "AGENT_LOOP",
          config: { inlineConfig: { profileId: "profile-xml" } },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults).toHaveLength(2);
    });
  });

  // ===========================================================================
  // Scenario C: Nodes with different protocols — inconsistent
  // ===========================================================================
  describe("nodes with different protocols", () => {
    it("should return consistent=false when nodes use different profiles with different formats", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: { profileId: "profile-native" },
        } as StaticNode,
        {
          id: "llm-2",
          type: "LLM",
          config: { profileId: "profile-xml" },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.errors[0]).toContain("Inconsistent");
    });

    it("should detect inconsistency when LLM and AGENT_LOOP nodes use different protocols", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: { profileId: "profile-native" },
        } as StaticNode,
        {
          id: "agent-1",
          type: "AGENT_LOOP",
          config: { inlineConfig: { profileId: "profile-xml" } },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  // ===========================================================================
  // Scenario D: Node-profile compatibility check
  // ===========================================================================
  describe("node-profile compatibility", () => {
    it("should detect incompatible node-level toolCallFormat with profile", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: {
            profileId: "profile-native",
            toolCallFormat: { format: "xml" },
          },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      // The node's format (xml) is incompatible with the profile's format (native)
      expect(result.nodeResults[0].valid).toBe(false);
      expect(result.nodeResults[0].error).toBeDefined();
    });

    it("should accept compatible node-level toolCallFormat with profile", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: {
            profileId: "profile-native",
            toolCallFormat: { format: "native" },
          },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.nodeResults[0].valid).toBe(true);
    });
  });

  // ===========================================================================
  // Scenario E: Edge cases
  // ===========================================================================
  describe("edge cases", () => {
    it("should handle nodes with no profileId gracefully", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: {},
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults[0].effectiveFormat).toBe("native");
    });

    it("should handle unresolvable profile gracefully", () => {
      const nodes: StaticNode[] = [
        {
          id: "llm-1",
          type: "LLM",
          config: { profileId: "non-existent-profile" },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults[0].effectiveFormat).toBe("native");
    });

    it("should handle AGENT_LOOP with profileId directly on config", () => {
      const nodes: StaticNode[] = [
        {
          id: "agent-1",
          type: "AGENT_LOOP",
          config: { profileId: "profile-native" },
        } as StaticNode,
      ];
      const result = validateWorkflowToolCallProtocolConsistency(nodes, profileResolver);
      expect(result.consistent).toBe(true);
      expect(result.nodeResults[0].effectiveFormat).toBe("native");
    });
  });
});