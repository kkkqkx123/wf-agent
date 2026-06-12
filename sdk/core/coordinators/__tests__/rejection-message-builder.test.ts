/**
 * Rejection Message Builder Tests
 */

import { describe, it, expect } from "vitest";
import { RejectionMessageBuilder } from "../rejection-message-builder.js";

describe("RejectionMessageBuilder", () => {
  describe("default configuration", () => {
    it("should use default template when no config provided", () => {
      const builder = new RejectionMessageBuilder();

      const message = builder.buildRejectionMessage("write_file");

      expect(message).toContain("write_file");
      expect(message).toContain("unavailable");
    });

    it("should include reason in default message", () => {
      const builder = new RejectionMessageBuilder();

      const message = builder.buildRejectionMessage("write_file", "Testing phase");

      expect(message).toContain("Testing phase");
    });
  });

  describe("custom templates", () => {
    it("should use tool-specific template", () => {
      const builder = new RejectionMessageBuilder({
        toolSpecificTemplates: {
          write_file: "Custom message for {{toolId}}: {{reason}}",
        },
      });

      const message = builder.buildRejectionMessage("write_file", "Not allowed");

      expect(message).toBe("Custom message for write_file: Not allowed");
    });

    it("should fall back to global template when tool-specific not found", () => {
      const builder = new RejectionMessageBuilder({
        globalDefaultTemplate: "Global: {{toolId}} - {{reason}}",
        toolSpecificTemplates: {
          write_file: "Write specific",
        },
      });

      const message = builder.buildRejectionMessage("delete_file", "Blocked");

      expect(message).toBe("Global: delete_file - Blocked");
    });
  });

  describe("user message hints", () => {
    it("should build hint with enabled and disabled tools", () => {
      const builder = new RejectionMessageBuilder();

      const hint = builder.buildUserMessageHint(["write_file"], ["delete_file"]);

      expect(hint).toContain("write_file");
      expect(hint).toContain("delete_file");
      expect(hint).toContain("disabled");
      expect(hint).toContain("available");
    });

    it("should return null when hints are disabled", () => {
      const builder = new RejectionMessageBuilder({
        injectUserMessageHint: false,
      });

      const hint = builder.buildUserMessageHint(["write_file"], []);

      expect(hint).toBeNull();
    });

    it("should return null when no changes", () => {
      const builder = new RejectionMessageBuilder();

      const hint = builder.buildUserMessageHint([], []);

      expect(hint).toBeNull();
    });

    it("should use custom hint template", () => {
      const builder = new RejectionMessageBuilder({
        userMessageHintTemplate: "Custom: +{{enabledTools}}, -{{disabledTools}}",
      });

      const hint = builder.buildUserMessageHint(["read_file"], ["write_file"]);

      expect(hint).toBe("Custom: +read_file, -write_file");
    });
  });

  describe("configuration updates", () => {
    it("should update configuration", () => {
      const builder = new RejectionMessageBuilder();

      builder.updateConfig({
        globalDefaultTemplate: "Updated: {{toolId}}",
      });

      const message = builder.buildRejectionMessage("test_tool");

      expect(message).toBe("Updated: test_tool");
    });

    it("should get current configuration", () => {
      const config = {
        globalDefaultTemplate: "Test template",
      };
      const builder = new RejectionMessageBuilder(config);

      const currentConfig = builder.getConfig();

      expect(currentConfig.globalDefaultTemplate).toBe("Test template");
    });
  });
});
