/**
 * Checkpoint metadata builder tests
 *
 * Tests the unified metadata building utilities
 */

import { describe, it, expect } from "vitest";
import {
  buildCheckpointMetadata,
  extractFormatVersion,
  extractCreatedAt,
} from "../metadata-builder.js";
import { CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";

describe("Checkpoint Metadata Builder", () => {
  describe("buildCheckpointMetadata", () => {
    it("should build metadata with all fields", () => {
      const options = {
        description: "Test checkpoint",
        tags: ["test", "manual"],
        customFields: { author: "test-user" },
      };

      const metadata = buildCheckpointMetadata(options);

      expect(metadata).toBeDefined();
      expect(metadata?.description).toBe("Test checkpoint");
      expect(metadata?.tags).toEqual(["test", "manual"]);
      expect(metadata?.customFields?.author).toBe("test-user");
      expect(metadata?.customFields?.formatVersion).toBe(CURRENT_CHECKPOINT_FORMAT_VERSION);
      expect(typeof metadata?.customFields?.createdAt).toBe("number");
    });

    it("should merge with existing metadata", () => {
      const options = {
        metadata: {
          description: "Original description",
          customFields: { originalField: "value" },
        },
        description: "Updated description",
        customFields: { newField: "new-value" },
      };

      const metadata = buildCheckpointMetadata(options);

      expect(metadata?.description).toBe("Updated description");
      expect(metadata?.customFields?.originalField).toBe("value");
      expect(metadata?.customFields?.newField).toBe("new-value");
    });

    it("should add version information automatically", () => {
      const metadata = buildCheckpointMetadata({
        description: "Test",
      });

      expect(metadata?.customFields?.formatVersion).toBe(CURRENT_CHECKPOINT_FORMAT_VERSION);
      expect(typeof metadata?.customFields?.createdAt).toBe("number");
    });

    it("should return undefined for empty options", () => {
      const metadata = buildCheckpointMetadata();
      expect(metadata).toBeUndefined();
    });
  });

  describe("extractFormatVersion", () => {
    it("should extract format version from metadata", () => {
      const metadata = {
        customFields: { formatVersion: "1.0.0" },
      };

      const version = extractFormatVersion(metadata);
      expect(version).toBe("1.0.0");
    });

    it("should return current version as default", () => {
      const version = extractFormatVersion();
      expect(version).toBe(CURRENT_CHECKPOINT_FORMAT_VERSION);
    });

    it("should return current version for missing field", () => {
      const metadata = { customFields: {} };
      const version = extractFormatVersion(metadata);
      expect(version).toBe(CURRENT_CHECKPOINT_FORMAT_VERSION);
    });
  });

  describe("extractCreatedAt", () => {
    it("should extract creation timestamp from metadata", () => {
      const timestamp = Date.now();
      const metadata = {
        customFields: { createdAt: timestamp },
      };

      const createdAt = extractCreatedAt(metadata);
      expect(createdAt).toBe(timestamp);
    });

    it("should return current time as default", () => {
      const before = Date.now();
      const createdAt = extractCreatedAt();
      const after = Date.now();

      expect(createdAt).toBeGreaterThanOrEqual(before);
      expect(createdAt).toBeLessThanOrEqual(after);
    });

    it("should return current time for missing field", () => {
      const before = Date.now();
      const metadata = { customFields: {} };
      const createdAt = extractCreatedAt(metadata);
      const after = Date.now();

      expect(createdAt).toBeGreaterThanOrEqual(before);
      expect(createdAt).toBeLessThanOrEqual(after);
    });

    it("should ignore non-numeric creation timestamp", () => {
      const metadata = {
        customFields: { createdAt: "not-a-number" },
      };

      const before = Date.now();
      const createdAt = extractCreatedAt(metadata);
      const after = Date.now();

      expect(createdAt).toBeGreaterThanOrEqual(before);
      expect(createdAt).toBeLessThanOrEqual(after);
    });
  });

  describe("Metadata Consistency", () => {
    it("should consistently add version information", () => {
      const metadata1 = buildCheckpointMetadata({ description: "Test 1" });
      const metadata2 = buildCheckpointMetadata({ description: "Test 2" });

      expect(metadata1?.customFields?.formatVersion).toBe(
        metadata2?.customFields?.formatVersion,
      );
    });

    it("should preserve custom fields while adding version info", () => {
      const customFields = {
        userId: "user-123",
        context: "manual-checkpoint",
        environment: "production",
      };

      const metadata = buildCheckpointMetadata({
        customFields,
      });

      expect(metadata?.customFields?.userId).toBe("user-123");
      expect(metadata?.customFields?.context).toBe("manual-checkpoint");
      expect(metadata?.customFields?.environment).toBe("production");
      expect(metadata?.customFields?.formatVersion).toBe(CURRENT_CHECKPOINT_FORMAT_VERSION);
    });
  });
});
