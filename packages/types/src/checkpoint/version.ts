/**
 * Checkpoint Format Version Management
 *
 * Provides version tracking, compatibility validation, and migration
 * for checkpoint format evolution.
 */

/**
 * Checkpoint format version
 * Format: MAJOR.MINOR
 * - MAJOR: Breaking changes require migration
 * - MINOR: Backward compatible changes
 */
export interface CheckpointFormatVersion {
  major: number;
  minor: number;
}

/**
 * Checkpoint format version info
 */
export const CHECKPOINT_FORMAT_VERSIONS = {
  V1_0: { major: 1, minor: 0 },
  V1_1: { major: 1, minor: 1 },
  V2_0: { major: 2, minor: 0 },
} as const;

/**
 * Current supported format version
 */
export const CURRENT_CHECKPOINT_FORMAT_VERSION: CheckpointFormatVersion = CHECKPOINT_FORMAT_VERSIONS.V1_0;

/**
 * Version compatibility result
 */
export interface VersionCompatibility {
  compatible: boolean;
  requiresMigration: boolean;
  targetVersion?: CheckpointFormatVersion;
  reason?: string;
}

/**
 * Version migration result
 */
export interface VersionMigrationResult<T = unknown> {
  success: boolean;
  fromVersion: CheckpointFormatVersion;
  toVersion: CheckpointFormatVersion;
  migratedData: T;
  errors?: string[];
}

/**
 * Migration handler for a specific version transition
 */
export type VersionMigrationHandler = (
  data: unknown,
  fromVersion: CheckpointFormatVersion,
) => Promise<unknown>;

/**
 * Version migration registry
 */
export interface VersionMigrationRegistry {
  [key: string]: VersionMigrationHandler;
}

/**
 * Checkpoint version metadata (included in checkpoint)
 */
export interface CheckpointVersionMetadata {
  /** Format version of this checkpoint */
  formatVersion: CheckpointFormatVersion;
  /** Schema version for entity-specific data */
  schemaVersion?: string;
  /** Creation timestamp */
  createdAt: number;
}

/**
 * Version compatibility rules
 */
export const COMPATIBILITY_RULES = {
  /**
   * Check if a version can be read by current system
   */
  canRead: (checkpointVersion: CheckpointFormatVersion, currentVersion: CheckpointFormatVersion): boolean => {
    // Can read same MAJOR version or older MINOR versions
    if (checkpointVersion.major === currentVersion.major) {
      return checkpointVersion.minor <= currentVersion.minor;
    }
    // Cannot read future major versions
    if (checkpointVersion.major > currentVersion.major) {
      return false;
    }
    // Can read older major versions (with migration)
    return true;
  },

  /**
   * Check if migration is needed
   */
  needsMigration: (checkpointVersion: CheckpointFormatVersion, currentVersion: CheckpointFormatVersion): boolean => {
    return checkpointVersion.major !== currentVersion.major || checkpointVersion.minor !== currentVersion.minor;
  },

  /**
   * Get version distance for migration routing
   */
  getVersionDistance: (
    fromVersion: CheckpointFormatVersion,
    toVersion: CheckpointFormatVersion,
  ): number => {
    const majorDiff = Math.abs(toVersion.major - fromVersion.major);
    const minorDiff = Math.abs(toVersion.minor - fromVersion.minor);
    return majorDiff * 100 + minorDiff;
  },
} as const;

/**
 * Version formatter utility
 */
export const versionFormatter = {
  toString: (version: CheckpointFormatVersion): string => {
    return `${version.major}.${version.minor}`;
  },

  fromString: (versionStr: string): CheckpointFormatVersion | null => {
    const match = versionStr.match(/^(\d+)\.(\d+)$/);
    if (!match) return null;
    return { major: parseInt(match[1]!, 10), minor: parseInt(match[2]!, 10) };
  },

  compare: (a: CheckpointFormatVersion, b: CheckpointFormatVersion): number => {
    if (a.major !== b.major) return a.major - b.major;
    return a.minor - b.minor;
  },
} as const;
