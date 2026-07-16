/**
 * AgentLoopValidator Protocol Tests
 *
 * Tests for validateAgentToolCallProtocol covering:
 * - No explicit config (defaults to native)
 * - Definition and profile both have explicit config, compatible
 * - Definition and profile both have explicit config, incompatible
 * - Definition has config, profile does not
 * - Edge cases
 */
import { describe, it, expect } from "vitest";
import { validateAgentToolCallProtocol } from "../agent-loop-validator.js";
import type { LLMProfile } from "@wf-agent/types";
import type { AgentLoopConfigFile } from "../../api/shared/config/types.js";

describe("validateAgentToolCallProtocol", () => {
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
  // Scenario A: No explicit config — defaults to native
  // ===========================================================================
  describe("no explicit config", () => {
    it("should return valid=true when neither definition nor profile has toolCallFormat", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        profileId: "profile-no-format",
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("should return valid=true when definition has no toolCallFormat and profile is not found", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        profileId: "non-existent",
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });
  });

  // ===========================================================================
  // Scenario B: Definition and profile both have explicit config — compatible
  // ===========================================================================
  describe("compatible explicit configs", () => {
    it("should return valid=true when both definition and profile use the same format", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        profileId: "profile-native",
        toolCallFormat: { format: "native" },
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });
  });

  // ===========================================================================
  // Scenario C: Definition and profile both have explicit config — incompatible
  // ===========================================================================
  describe("incompatible explicit configs", () => {
    it("should return valid=false with error when definition format differs from profile format", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        profileId: "profile-native",
        toolCallFormat: { format: "xml" },
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.errors[0]).toContain("mismatch");
    });
  });

  // ===========================================================================
  // Scenario D: Definition has config, profile does not
  // ===========================================================================
  describe("definition has config, profile does not", () => {
    it("should return valid=true with warning when definition has toolCallFormat but profile has none", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        profileId: "profile-no-format",
        toolCallFormat: { format: "xml" },
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(result.warnings[0]).toContain("no explicit toolCallFormat");
    });
  });

  // ===========================================================================
  // Scenario E: Edge cases
  // ===========================================================================
  describe("edge cases", () => {
    it("should handle definition with no profileId", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
        toolCallFormat: { format: "native" },
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
    });

    it("should handle definition with no profileId and no toolCallFormat", () => {
      const definition: AgentLoopConfigFile = {
        id: "agent-1",
      } as AgentLoopConfigFile;
      const result = validateAgentToolCallProtocol(definition, profileResolver);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
      expect(result.warnings).toHaveLength(0);
    });
  });
});