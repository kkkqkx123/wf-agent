/**
 * Data Consistency Validator
 *
 * Validates and reports data consistency between in-memory registry
 * and persistent storage.
 *
 * Use cases:
 * - Verify data integrity after recovery
 * - Detect orphaned or missing data
 * - Diagnose storage synchronization issues
 */

export interface ConsistencyIssue {
  type: "orphaned_in_storage" | "missing_in_storage";
  entityId: string;
  description: string;
  severity: "error" | "warning";
}

export interface ConsistencyReport {
  timestamp: number;
  registryName: string;
  memoryEntityCount: number;
  storageEntityCount: number;
  consistent: boolean;
  issues: ConsistencyIssue[];
  summary: {
    orphanedInStorage: string[]; // Exist in storage but not in memory
    missingInStorage: string[]; // Exist in memory but not in storage
  };
}

/**
 * Validates consistency between memory and storage
 */
export class DataConsistencyValidator {
  /**
   * Verify data consistency in a registry
   *
   * Compares entity IDs in memory with those in storage:
   * - Detects entities in storage but not in memory (orphaned data)
   * - Detects entities in memory but not in storage (persistence failed)
   * - Detects count mismatches
   *
   * @param registryName Name of the registry being verified
   * @param memoryIds IDs stored in memory
   * @param storageIds IDs stored in persistent storage
   * @returns Consistency report with any issues found
   */
  verify(
    registryName: string,
    memoryIds: Set<string>,
    storageIds: Set<string>,
  ): ConsistencyReport {
    const issues: ConsistencyIssue[] = [];
    const orphanedInStorage: string[] = [];
    const missingInStorage: string[] = [];

    // Check for orphaned data in storage (exists in storage but not in memory)
    for (const id of storageIds) {
      if (!memoryIds.has(id)) {
        orphanedInStorage.push(id);
        issues.push({
          type: "orphaned_in_storage",
          entityId: id,
          description: `Entity exists in storage but not in memory (may indicate incomplete cleanup)`,
          severity: "warning",
        });
      }
    }

    // Check for missing data in storage (exists in memory but not in storage)
    for (const id of memoryIds) {
      if (!storageIds.has(id)) {
        missingInStorage.push(id);
        issues.push({
          type: "missing_in_storage",
          entityId: id,
          description: `Entity exists in memory but not in storage (persistence may have failed)`,
          severity: "error",
        });
      }
    }

    const consistent =
      orphanedInStorage.length === 0 &&
      missingInStorage.length === 0 &&
      memoryIds.size === storageIds.size;

    const report: ConsistencyReport = {
      timestamp: Date.now(),
      registryName,
      memoryEntityCount: memoryIds.size,
      storageEntityCount: storageIds.size,
      consistent,
      issues,
      summary: {
        orphanedInStorage,
        missingInStorage,
      },
    };

    return report;
  }

  /**
   * Generate human-readable consistency report
   */
  generateReport(report: ConsistencyReport): string {
    let text = `\n=== Data Consistency Report ===\n`;
    text += `Registry: ${report.registryName}\n`;
    text += `Timestamp: ${new Date(report.timestamp).toISOString()}\n`;
    text += `Status: ${report.consistent ? "✅ CONSISTENT" : "❌ INCONSISTENT"}\n\n`;

    text += `Summary:\n`;
    text += `  Memory entities: ${report.memoryEntityCount}\n`;
    text += `  Storage entities: ${report.storageEntityCount}\n`;

    if (report.issues.length > 0) {
      text += `\nIssues (${report.issues.length}):\n`;
      for (const issue of report.issues) {
        const icon = issue.severity === "error" ? "❌" : "⚠️";
        text += `  ${icon} [${issue.type}] ${issue.entityId}\n`;
        text += `     ${issue.description}\n`;
      }
    }

    if (report.summary.orphanedInStorage.length > 0) {
      text += `\nOrphaned in Storage (${report.summary.orphanedInStorage.length}):\n`;
      for (const id of report.summary.orphanedInStorage) {
        text += `  - ${id}\n`;
      }
    }

    if (report.summary.missingInStorage.length > 0) {
      text += `\nMissing in Storage (${report.summary.missingInStorage.length}):\n`;
      for (const id of report.summary.missingInStorage) {
        text += `  - ${id}\n`;
      }
    }

    text += `\n==============================\n`;
    return text;
  }
}
