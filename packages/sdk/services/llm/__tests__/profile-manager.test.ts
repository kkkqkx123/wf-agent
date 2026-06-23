/**
 * ProfileManager Unit Tests
 * Tests for LLM Profile management
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ProfileManager } from "../profile-manager.js";
import { NotFoundError, ConfigurationValidationError } from "@wf-agent/types";
import type { LLMProfile } from "@wf-agent/types";

function createProfile(overrides: Partial<LLMProfile> = {}): LLMProfile {
  return {
    id: "test-id",
    name: "Test Profile",
    provider: "OPENAI_CHAT",
    model: "gpt-4",
    apiKey: "test-api-key",
    parameters: {},
    ...overrides,
  };
}

describe("ProfileManager", () => {
  let manager: ProfileManager;

  beforeEach(() => {
    manager = new ProfileManager();
  });

  describe("register", () => {
    it("should register a valid profile", () => {
      const profile = createProfile();
      manager.register(profile);
      expect(manager.has("test-id")).toBe(true);
      expect(manager.size()).toBe(1);
    });

    it("should set the first registered profile as default", () => {
      const profile = createProfile();
      manager.register(profile);
      expect(manager.getDefault()).toEqual(profile);
    });

    it("should not change default when registering additional profiles", () => {
      manager.register(createProfile({ id: "first", name: "First" }));
      manager.register(createProfile({ id: "second", name: "Second" }));
      expect(manager.getDefault()?.id).toBe("first");
    });

    it("should throw ConfigurationValidationError if id is missing", () => {
      expect(() => manager.register(createProfile({ id: "" as any }))).toThrow(
        ConfigurationValidationError,
      );
    });

    it("should throw ConfigurationValidationError if id is undefined", () => {
      const profile = createProfile();
      delete (profile as any).id;
      expect(() => manager.register(profile)).toThrow(ConfigurationValidationError);
    });

    it("should throw ConfigurationValidationError if name is missing", () => {
      const profile = createProfile();
      delete (profile as any).name;
      expect(() => manager.register(profile)).toThrow(ConfigurationValidationError);
    });

    it("should throw ConfigurationValidationError if provider is missing", () => {
      const profile = createProfile();
      delete (profile as any).provider;
      expect(() => manager.register(profile)).toThrow(ConfigurationValidationError);
    });

    it("should throw ConfigurationValidationError if model is missing", () => {
      const profile = createProfile();
      delete (profile as any).model;
      expect(() => manager.register(profile)).toThrow(ConfigurationValidationError);
    });

    it("should throw ConfigurationValidationError if apiKey is missing", () => {
      const profile = createProfile();
      delete (profile as any).apiKey;
      expect(() => manager.register(profile)).toThrow(ConfigurationValidationError);
    });

    it("should overwrite existing profile with the same id", () => {
      manager.register(createProfile({ id: "same-id", name: "Original" }));
      manager.register(createProfile({ id: "same-id", name: "Updated" }));
      expect(manager.get("same-id")?.name).toBe("Updated");
    });
  });

  describe("get", () => {
    it("should return profile by id", () => {
      const profile = createProfile();
      manager.register(profile);
      expect(manager.get("test-id")).toEqual(profile);
    });

    it("should return undefined for non-existent profile", () => {
      expect(manager.get("non-existent")).toBeUndefined();
    });

    it("should return default profile when no id provided", () => {
      manager.register(createProfile({ id: "default-profile" }));
      expect(manager.get()).toEqual(expect.objectContaining({ id: "default-profile" }));
    });

    it("should return undefined when no id provided and no default set", () => {
      expect(manager.get()).toBeUndefined();
    });
  });

  describe("getDefault", () => {
    it("should return the default profile", () => {
      manager.register(createProfile({ id: "first" }));
      manager.register(createProfile({ id: "second" }));
      expect(manager.getDefault()?.id).toBe("first");
    });

    it("should return undefined when no profiles exist", () => {
      expect(manager.getDefault()).toBeUndefined();
    });
  });

  describe("setDefault", () => {
    it("should set a different profile as default", () => {
      manager.register(createProfile({ id: "first" }));
      manager.register(createProfile({ id: "second" }));
      manager.setDefault("second");
      expect(manager.getDefault()?.id).toBe("second");
    });

    it("should throw NotFoundError for non-existent profile", () => {
      expect(() => manager.setDefault("non-existent")).toThrow(NotFoundError);
    });
  });

  describe("remove", () => {
    it("should remove a profile", () => {
      manager.register(createProfile());
      manager.remove("test-id");
      expect(manager.has("test-id")).toBe(false);
      expect(manager.size()).toBe(0);
    });

    it("should update default when removing the default profile", () => {
      manager.register(createProfile({ id: "first" }));
      manager.register(createProfile({ id: "second" }));
      manager.remove("first");
      expect(manager.getDefault()?.id).toBe("second");
    });

    it("should set default to null when removing last profile", () => {
      manager.register(createProfile({ id: "only" }));
      manager.remove("only");
      expect(manager.getDefault()).toBeUndefined();
    });

    it("should not fail when removing non-existent profile", () => {
      expect(() => manager.remove("non-existent")).not.toThrow();
    });
  });

  describe("list", () => {
    it("should list all profiles", () => {
      const p1 = createProfile({ id: "first", name: "First" });
      const p2 = createProfile({ id: "second", name: "Second" });
      manager.register(p1);
      manager.register(p2);
      const list = manager.list();
      expect(list).toHaveLength(2);
      expect(list).toContainEqual(p1);
      expect(list).toContainEqual(p2);
    });

    it("should return empty array when no profiles", () => {
      expect(manager.list()).toEqual([]);
    });
  });

  describe("clear", () => {
    it("should clear all profiles", () => {
      manager.register(createProfile({ id: "first" }));
      manager.register(createProfile({ id: "second" }));
      manager.clear();
      expect(manager.size()).toBe(0);
      expect(manager.list()).toEqual([]);
      expect(manager.getDefault()).toBeUndefined();
    });
  });

  describe("has", () => {
    it("should return true for existing profile", () => {
      manager.register(createProfile());
      expect(manager.has("test-id")).toBe(true);
    });

    it("should return false for non-existing profile", () => {
      expect(manager.has("non-existent")).toBe(false);
    });
  });

  describe("size", () => {
    it("should return the number of profiles", () => {
      expect(manager.size()).toBe(0);
      manager.register(createProfile({ id: "first" }));
      expect(manager.size()).toBe(1);
      manager.register(createProfile({ id: "second" }));
      expect(manager.size()).toBe(2);
    });
  });

  describe("edge cases", () => {
    it("should handle re-registering the same profile", () => {
      manager.register(createProfile({ id: "dup" }));
      manager.register(createProfile({ id: "dup", name: "Updated" }));
      expect(manager.size()).toBe(1);
      expect(manager.get("dup")?.name).toBe("Updated");
    });

    it("should not change default when re-registering same id", () => {
      manager.register(createProfile({ id: "a" }));
      manager.register(createProfile({ id: "b" }));
      manager.setDefault("b");
      // Re-register "a"
      manager.register(createProfile({ id: "a", name: "Updated A" }));
      expect(manager.getDefault()?.id).toBe("b");
    });
  });
});
