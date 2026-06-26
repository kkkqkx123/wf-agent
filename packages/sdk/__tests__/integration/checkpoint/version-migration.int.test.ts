/**
 * Version Migration Integration Tests
 *
 * Tests version compatibility and migration workflows.
 * Covers: CP-INT-12 through CP-INT-15
 */

import { describe, it, expect, beforeEach } from "vitest";
import { CheckpointVersionManager } from "@sdk/shared/checkpoint/checkpoint-version-manager.js";
import type {
  CheckpointFormatVersion,
  VersionCompatibility,
  VersionMigrationResult,
} from "@wf-agent/types";

function createTestLogger() {
  const logs: string[] = [];
  return {
    logger: {
      debug: (msg: string) => logs.push(`DEBUG: ${msg}`),
      info: (msg: string) => logs.push(`INFO: ${msg}`),
      warn: (msg: string) => logs.push(`WARN: ${msg}`),
      error: (msg: string) => logs.push(`ERROR: ${msg}`),
    },
    logs,
  };
}

describe("Version Migration Integration", () => {
  describe("CP-INT-12: compatibility checks", () => {
    it("should return compatible for same version", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      const result = vm.checkCompatibility({ major: 1, minor: 0 });
      expect(result.compatible).toBe(true);
      expect(result.requiresMigration).toBe(false);
    });

    it("should return compatible for higher patch version", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 1 });

      const result = vm.checkCompatibility({ major: 1, minor: 0 });
      expect(result.compatible).toBe(true);
    });

    it("should return incompatible for higher minor version", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      const result = vm.checkCompatibility({ major: 1, minor: 2 });
      expect(result.compatible).toBe(false);
    });

    it("should return incompatible for major version mismatch", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      const result = vm.checkCompatibility({ major: 2, minor: 0 });
      expect(result.compatible).toBe(false);
    });

    it("should detect migration requirement", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 1 });

      const result = vm.checkCompatibility({ major: 1, minor: 0 });
      expect(result.compatible).toBe(true);
      expect(result.requiresMigration).toBe(true);
    });
  });

  describe("CP-INT-13: migration path calculation", () => {
    it("should calculate correct migration path for v1.0 to v1.1", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 1 });

      const migrationCalls: string[] = [];
      vm.registerMigration("1.0->1.1", async (data: unknown) => {
        migrationCalls.push("1.0->1.1");
        return data;
      });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(true);
      expect(migrationCalls).toEqual(["1.0->1.1"]);
    });

    it("should calculate correct migration path for v1.0 to v2.0", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 2, minor: 0 });

      const migrationCalls: string[] = [];
      vm.registerMigration("1.0->1.1", async (data: unknown) => {
        migrationCalls.push("1.0->1.1");
        return data;
      });
      vm.registerMigration("1.1->2.0", async (data: unknown) => {
        migrationCalls.push("1.1->2.0");
        return data;
      });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(true);
      expect(migrationCalls).toEqual(["1.0->1.1", "1.1->2.0"]);
    });

    it("should skip migration when already at current version", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      const migrationCalls: string[] = [];
      vm.registerMigration("1.0->1.1", async (data: unknown) => {
        migrationCalls.push("1.0->1.1");
        return data;
      });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(true);
      expect(migrationCalls).toEqual([]);
    });
  });

  describe("CP-INT-14: migration failure handling", () => {
    it("should return failure result when migration handler throws", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 1 });

      vm.registerMigration("1.0->1.1", async () => {
        throw new Error("Migration failed");
      });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(false);
      expect(result.errors).toContain("Migration failed");
    });

    it("should return failure when no migration path exists", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 3, minor: 0 });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(false);
    });
  });

  describe("CP-INT-15: version metadata management", () => {
    it("should add version metadata to checkpoint", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      const checkpoint: Record<string, unknown> = { id: "cp-1", type: "FULL" };
      const meta = vm.addVersionMetadata(checkpoint, "1.0.0");

      expect(meta.formatVersion).toEqual({ major: 1, minor: 0 });
      expect(meta.schemaVersion).toBe("1.0.0");
      expect((checkpoint.metadata as Record<string, unknown>).formatVersion).toBeDefined();
    });

    it("should validate version metadata correctly", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      expect(vm.validateVersionMetadata({})).toBe(false);
      expect(vm.validateVersionMetadata({ metadata: {} })).toBe(false);
      expect(vm.validateVersionMetadata({ metadata: { formatVersion: { major: 1, minor: 0 } } })).toBe(true);
    });

    it("should update current version", () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 0 });

      vm.setCurrentVersion({ major: 2, minor: 0 });
      expect(vm.getCurrentVersion()).toEqual({ major: 2, minor: 0 });
    });
  });

  describe("CP-INT-16: custom migration handlers", () => {
    it("should allow registering custom migration handlers", async () => {
      const { logger } = createTestLogger();
      const vm = new CheckpointVersionManager(logger, { major: 1, minor: 1 });

      const customCalls: unknown[] = [];
      vm.registerMigration("1.0->1.1", async (data: unknown) => {
        customCalls.push(data);
        return { ...(data as object), migrated: true };
      });

      const checkpoint = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: { formatVersion: { major: 1, minor: 0 } },
      };

      const result = await vm.migrateCheckpoint(checkpoint);
      expect(result.success).toBe(true);
      expect(customCalls.length).toBe(1);
    });
  });
});
