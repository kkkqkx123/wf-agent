/**
 * Migration Manager
 * 
 * Provides version migration framework with version-specific transformers.
 * Enables safe schema evolution and backward compatibility for serialized snapshots.
 */

import type { SnapshotBase } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "MigrationManager" });

/**
 * Migration step definition
 */
export interface MigrationStep<T extends SnapshotBase = SnapshotBase> {
  /** Source version */
  fromVersion: number;
  /** Target version */
  toVersion: number;
  /** Migration function that transforms the snapshot */
  migrate(snapshot: any): T | Promise<T>;
  /** Optional description of what this migration does */
  description?: string;
}

/**
 * Migration result
 */
export interface MigrationResult<T extends SnapshotBase> {
  /** The migrated snapshot */
  snapshot: T;
  /** Number of migrations applied */
  migrationsApplied: number;
  /** Versions that were migrated through */
  versionPath: number[];
}

/**
 * Migration Manager
 * 
 * Manages migration steps for different entity types and provides
 * automatic version migration during deserialization.
 */
export class MigrationManager {
  private static instance: MigrationManager | null = null;
  private migrations: Map<string, MigrationStep[]> = new Map();

  private constructor() {}

  /**
   * Get singleton instance
   */
  static getInstance(): MigrationManager {
    if (!MigrationManager.instance) {
      MigrationManager.instance = new MigrationManager();
    }
    return MigrationManager.instance;
  }

  /**
   * Reset singleton instance (useful for testing)
   */
  static resetInstance(): void {
    MigrationManager.instance = null;
  }

  /**
   * Register migration steps for an entity type
   * 
   * @param entityType The entity type identifier
   * @param steps Array of migration steps (should be ordered by fromVersion)
   */
  registerMigrations<T extends SnapshotBase>(entityType: string, steps: MigrationStep<T>[]): void {
    // Sort steps by fromVersion to ensure correct order
    const sortedSteps = [...steps].sort((a, b) => a.fromVersion - b.fromVersion);
    this.migrations.set(entityType, sortedSteps);
    
    logger.info(`Registered ${sortedSteps.length} migration steps for entity type: ${entityType}`, {
      entityType,
      versions: sortedSteps.map(s => `${s.fromVersion} -> ${s.toVersion}`),
    });
  }

  /**
   * Get registered migrations for an entity type
   */
  getMigrations(entityType: string): MigrationStep[] {
    return this.migrations.get(entityType) || [];
  }

  /**
   * Check if migrations are registered for an entity type
   */
  hasMigrations(entityType: string): boolean {
    return this.migrations.has(entityType);
  }

  /**
   * Migrate a snapshot to the target version
   * 
   * Applies all necessary migration steps in sequence to upgrade the snapshot
   * from its current version to the target version.
   * 
   * @param snapshot The snapshot to migrate
   * @param entityType The entity type
   * @param targetVersion The target version to migrate to
   * @returns Migration result with the upgraded snapshot
   */
  async migrate<T extends SnapshotBase>(
    snapshot: T,
    entityType: string,
    targetVersion: number,
  ): Promise<MigrationResult<T>> {
    const currentVersion = snapshot._version;
    
    // No migration needed
    if (currentVersion === targetVersion) {
      logger.debug("No migration needed", {
        entityType,
        currentVersion,
        targetVersion,
      });
      
      return {
        snapshot,
        migrationsApplied: 0,
        versionPath: [currentVersion],
      };
    }

    // Cannot downgrade
    if (currentVersion > targetVersion) {
      throw new Error(
        `Cannot downgrade ${entityType} from version ${currentVersion} to ${targetVersion}`,
      );
    }

    const steps = this.migrations.get(entityType) || [];
    
    if (steps.length === 0) {
      logger.warn(`No migrations registered for entity type: ${entityType}`, {
        entityType,
        currentVersion,
        targetVersion,
      });
      
      // If no migrations are registered, just update the version
      return {
        snapshot: { ...snapshot, _version: targetVersion } as T,
        migrationsApplied: 0,
        versionPath: [currentVersion, targetVersion],
      };
    }

    let current: any = snapshot;
    const versionPath: number[] = [currentVersion];
    let migrationsApplied = 0;

    logger.info(`Starting migration`, {
      entityType,
      fromVersion: currentVersion,
      toVersion: targetVersion,
      availableSteps: steps.length,
    });

    // Apply migrations sequentially
    for (const step of steps) {
      // Check if this step is needed
      if (current._version >= step.toVersion) {
        continue; // Already at or beyond this step's target
      }

      if (current._version < step.fromVersion) {
        continue; // Not yet at this step's source version
      }

      try {
        logger.debug(`Applying migration step`, {
          entityType,
          fromVersion: step.fromVersion,
          toVersion: step.toVersion,
          description: step.description,
        });

        current = await step.migrate(current);
        versionPath.push(current._version);
        migrationsApplied++;

        logger.debug(`Migration step applied successfully`, {
          entityType,
          newVersion: current._version,
        });

        // Stop if we've reached the target version
        if (current._version >= targetVersion) {
          break;
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        logger.error(`Migration step failed`, {
          entityType,
          fromVersion: step.fromVersion,
          toVersion: step.toVersion,
          error: errorMessage,
          snapshot: JSON.stringify(current).substring(0, 500), // Log first 500 chars for debugging
        });
        
        throw new Error(
          `Failed to migrate ${entityType} from version ${step.fromVersion} to ${step.toVersion}: ${errorMessage}`,
          { cause: error },
        );
      }
    }

    // Ensure final version matches target
    if (current._version !== targetVersion) {
      logger.warn(`Migration did not reach target version`, {
        entityType,
        expectedVersion: targetVersion,
        actualVersion: current._version,
        versionPath,
      });
      
      // Force set the target version
      current = { ...current, _version: targetVersion };
    }

    logger.info(`Migration completed`, {
      entityType,
      fromVersion: snapshot._version,
      toVersion: current._version,
      migrationsApplied,
      versionPath,
    });

    return {
      snapshot: current as T,
      migrationsApplied,
      versionPath,
    };
  }

  /**
   * Get the latest version for an entity type based on registered migrations
   */
  getLatestVersion(entityType: string): number {
    const steps = this.migrations.get(entityType) || [];
    
    if (steps.length === 0) {
      return 1; // Default to version 1 if no migrations
    }

    // Return the highest toVersion from all steps
    return Math.max(...steps.map(s => s.toVersion));
  }

  /**
   * Clear all registered migrations
   */
  clearMigrations(): void {
    this.migrations.clear();
    logger.info("Cleared all registered migrations");
  }

  /**
   * Create a helper function for building migration steps
   */
  static createMigrationStep<T extends SnapshotBase>(
    fromVersion: number,
    toVersion: number,
    migrate: (snapshot: any) => T | Promise<T>,
    description?: string,
  ): MigrationStep<T> {
    return {
      fromVersion,
      toVersion,
      migrate,
      description,
    };
  }
}

/**
 * Convenience function to get the migration manager
 */
export function getMigrationManager(): MigrationManager {
  return MigrationManager.getInstance();
}
