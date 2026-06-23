/**
 * Checkpoint Version Manager
 *
 * Handles version compatibility checks, validation, and migrations
 * for checkpoint format evolution.
 */

import type {
  CheckpointFormatVersion,
  VersionCompatibility,
  VersionMigrationResult,
  VersionMigrationHandler,
  VersionMigrationRegistry,
  CheckpointVersionMetadata,
} from "@wf-agent/types";
import {
  CURRENT_CHECKPOINT_FORMAT_VERSION,
  COMPATIBILITY_RULES,
  versionFormatter,
} from "@wf-agent/types";

interface Logger {
  debug(msg: string, context?: Record<string, unknown>): void;
  info(msg: string, context?: Record<string, unknown>): void;
  warn(msg: string, context?: Record<string, unknown>): void;
  error(msg: string, context?: Record<string, unknown>): void;
}

/**
 * Checkpoint Version Manager
 * Manages version compatibility, validation, and migration
 */
export class CheckpointVersionManager {
  private readonly logger: Logger;
  private migrationRegistry: VersionMigrationRegistry = {};
  private currentVersion: CheckpointFormatVersion;

  constructor(logger: Logger, currentVersion: CheckpointFormatVersion = CURRENT_CHECKPOINT_FORMAT_VERSION) {
    this.logger = logger;
    this.currentVersion = currentVersion;
    this.initializeDefaultMigrations();
  }

  /**
   * Initialize default migration handlers (v1.0 -> v1.1, etc.)
   */
  private initializeDefaultMigrations(): void {
    // v1.0 -> v1.1: Add new fields with defaults
    this.registerMigration("1.0->1.1", async (data: unknown) => {
      this.logger.debug("Migrating checkpoint from 1.0 to 1.1");
      return {
        ...(typeof data === "object" && data !== null && !Array.isArray(data) ? data : {}),
        // Add any new fields with sensible defaults
        _migrationApplied: true,
      };
    });

    // v1.1 -> v2.0: Major version change requires structural migration
    this.registerMigration("1.1->2.0", async (data: unknown) => {
      this.logger.debug("Migrating checkpoint from 1.1 to 2.0");
      // This is a major version change - significant restructuring
      return {
        ...(typeof data === "object" && data !== null && !Array.isArray(data) ? data : {}),
        _migrationApplied: true,
        _majorVersionUpgrade: true,
      };
    });
  }

  /**
   * Register a custom migration handler
   */
  registerMigration(key: string, handler: VersionMigrationHandler): void {
    this.migrationRegistry[key] = handler;
    this.logger.debug("Migration handler registered", { key });
  }

  /**
   * Check version compatibility
   */
  checkCompatibility(checkpointVersion: CheckpointFormatVersion): VersionCompatibility {
    const canRead = COMPATIBILITY_RULES.canRead(checkpointVersion, this.currentVersion);

    if (!canRead) {
      return {
        compatible: false,
        requiresMigration: false,
        reason: `Checkpoint version ${versionFormatter.toString(checkpointVersion)} is not supported (current: ${versionFormatter.toString(this.currentVersion)})`,
      };
    }

    const needsMigration = COMPATIBILITY_RULES.needsMigration(checkpointVersion, this.currentVersion);

    return {
      compatible: true,
      requiresMigration: needsMigration,
      targetVersion: this.currentVersion,
      reason: needsMigration ? "Migration required to latest format" : "Direct compatible",
    };
  }

  /**
   * Validate checkpoint has required version metadata
   */
  validateVersionMetadata(checkpoint: unknown): boolean {
    const cp = checkpoint as Record<string, unknown>;
    if (!cp || typeof cp !== "object") {
      this.logger.warn("Invalid checkpoint: not an object");
      return false;
    }

    const metadata = cp['metadata'] as Record<string, unknown>;
    if (!metadata || !metadata['formatVersion']) {
      this.logger.warn("Invalid checkpoint: missing format version metadata");
      return false;
    }

    const version = metadata['formatVersion'] as Record<string, unknown>;
    if (typeof version['major'] !== "number" || typeof version['minor'] !== "number") {
      this.logger.warn("Invalid format version structure", { version });
      return false;
    }

    return true;
  }

  /**
   * Add version metadata to checkpoint
   */
  addVersionMetadata(checkpoint: unknown, schemaVersion?: string): CheckpointVersionMetadata {
    const cp = checkpoint as Record<string, unknown>;
    const metadata: CheckpointVersionMetadata = {
      formatVersion: this.currentVersion,
      schemaVersion,
      createdAt: Date.now(),
    };

    if (!cp['metadata']) {
      cp['metadata'] = {};
    }
    const cpMetadata = cp['metadata'] as Record<string, unknown>;
    cpMetadata['formatVersion'] = this.currentVersion;
    if (schemaVersion) {
      cpMetadata['schemaVersion'] = schemaVersion;
    }

    return metadata;
  }

  /**
   * Migrate checkpoint to current version
   */
  async migrateCheckpoint(checkpoint: unknown): Promise<VersionMigrationResult<unknown>> {
    const checkpointRecord = checkpoint as Record<string, unknown>;
    const metadataRecord = checkpointRecord?.['metadata'] as Record<string, unknown>;
    const sourceVersion = (metadataRecord?.['formatVersion'] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;

    if (versionFormatter.compare(sourceVersion, this.currentVersion) === 0) {
      // Already at current version
      return {
        success: true,
        fromVersion: sourceVersion,
        toVersion: this.currentVersion,
        migratedData: checkpoint,
      };
    }

    if (!COMPATIBILITY_RULES.canRead(sourceVersion, this.currentVersion)) {
      return {
        success: false,
        fromVersion: sourceVersion,
        toVersion: this.currentVersion,
        migratedData: checkpoint,
        errors: [
          `Cannot migrate from version ${versionFormatter.toString(sourceVersion)} to ${versionFormatter.toString(this.currentVersion)}`,
        ],
      };
    }

    try {
      let migratedData = checkpoint;
      const migrationPath = this.calculateMigrationPath(sourceVersion, this.currentVersion);

      this.logger.debug("Starting checkpoint migration", {
        from: versionFormatter.toString(sourceVersion),
        to: versionFormatter.toString(this.currentVersion),
        steps: migrationPath.length,
      });

      for (const step of migrationPath) {
        const handler = this.migrationRegistry[step];
        if (!handler) {
          throw new Error(`No migration handler for step: ${step}`);
        }

        migratedData = await handler(migratedData, sourceVersion);
      }

      // Update version metadata
      this.addVersionMetadata(migratedData);

      this.logger.info("Checkpoint migration completed successfully", {
        from: versionFormatter.toString(sourceVersion),
        to: versionFormatter.toString(this.currentVersion),
      });

      return {
        success: true,
        fromVersion: sourceVersion,
        toVersion: this.currentVersion,
        migratedData,
      };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.logger.error("Checkpoint migration failed", {
        from: versionFormatter.toString(sourceVersion),
        to: versionFormatter.toString(this.currentVersion),
        error: errorMsg,
      });

      return {
        success: false,
        fromVersion: sourceVersion,
        toVersion: this.currentVersion,
        migratedData: checkpoint,
        errors: [errorMsg],
      };
    }
  }

  /**
   * Calculate migration path from source to target version
   * Currently supports linear paths (v1.0 -> v1.1 -> v2.0)
   */
  private calculateMigrationPath(
    sourceVersion: CheckpointFormatVersion,
    targetVersion: CheckpointFormatVersion,
  ): string[] {
    const source = versionFormatter.toString(sourceVersion);
    const target = versionFormatter.toString(targetVersion);

    // Define all possible migrations as a chain
    const migrationChain = [
      { from: "1.0", to: "1.1", key: "1.0->1.1" },
      { from: "1.1", to: "2.0", key: "1.1->2.0" },
    ];

    const path: string[] = [];
    let current = source;

    while (current !== target) {
      const migration = migrationChain.find((m) => m.from === current);
      if (!migration) {
        throw new Error(`No migration path from ${current} to ${target}`);
      }
      path.push(migration.key);
      current = migration.to;
    }

    return path;
  }

  /**
   * Get current version
   */
  getCurrentVersion(): CheckpointFormatVersion {
    return this.currentVersion;
  }

  /**
   * Set current version
   */
  setCurrentVersion(version: CheckpointFormatVersion): void {
    this.currentVersion = version;
    this.logger.debug("Current checkpoint format version updated", {
      version: versionFormatter.toString(version),
    });
  }

  /**
   * Get version as string
   */
  getVersionString(version: CheckpointFormatVersion): string {
    return versionFormatter.toString(version);
  }
}
