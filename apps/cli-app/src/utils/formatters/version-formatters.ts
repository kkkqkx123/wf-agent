/**
 * Workflow Version Formatters
 * Format version management results for CLI output
 */

import type {
  WorkflowVersion,
  VersionDiff,
} from "../../adapters/workflow-version-adapter.js";

export function formatVersionList(versions: WorkflowVersion[]): string {
  if (versions.length === 0) {
    return "No versions found";
  }

  const lines: string[] = [];

  lines.push("");
  lines.push("WORKFLOW VERSIONS");
  lines.push("=================");
  lines.push("");

  for (const version of versions) {
    lines.push(`[Version] ${version.version}`);
    if (version.name) {
      lines.push(`   Name: ${version.name}`);
    }
    if (version.description) {
      lines.push(`   Description: ${version.description}`);
    }
    lines.push(`   Nodes: ${version.nodeCount}, Edges: ${version.edgeCount}`);
    lines.push(
      `   Created: ${new Date(version.createdAt).toLocaleString()}`
    );
    if (version.author) {
      lines.push(`   Author: ${version.author}`);
    }
    if (version.tags && version.tags.length > 0) {
      lines.push(`   Tags: ${version.tags.join(", ")}`);
    }
    lines.push("");
  }

  lines.push(`Total Versions: ${versions.length}`);
  lines.push("");

  return lines.join("\n");
}

export function formatVersionDiff(diff: VersionDiff): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("VERSION COMPARISON");
  lines.push("==================");
  lines.push(`Comparing: ${diff.version1} -> ${diff.version2}`);
  lines.push("");

  // Structure changes
  lines.push("STRUCTURAL CHANGES");
  lines.push("------------------");
  lines.push("");

  const nodeStatus = diff.nodeCountDiff > 0 ? "[Added]" : diff.nodeCountDiff < 0 ? "[Removed]" : "[No Change]";
  lines.push(
    `Nodes ${nodeStatus}: ${diff.nodeCountDiff > 0 ? "+" : ""}${diff.nodeCountDiff}`
  );

  const edgeStatus = diff.edgeCountDiff > 0 ? "[Added]" : diff.edgeCountDiff < 0 ? "[Removed]" : "[No Change]";
  lines.push(
    `Edges ${edgeStatus}: ${diff.edgeCountDiff > 0 ? "+" : ""}${diff.edgeCountDiff}`
  );

  lines.push("");

  // Detailed changes
  if (diff.nodesAdded.length > 0) {
    lines.push(`[NEW] Nodes Added (${diff.nodesAdded.length}):`);
    for (const node of diff.nodesAdded.slice(0, 5)) {
      lines.push(`  - ${node}`);
    }
    if (diff.nodesAdded.length > 5) {
      lines.push(`  ... and ${diff.nodesAdded.length - 5} more`);
    }
    lines.push("");
  }

  if (diff.nodesRemoved.length > 0) {
    lines.push(`[REMOVED] Nodes Removed (${diff.nodesRemoved.length}):`);
    for (const node of diff.nodesRemoved.slice(0, 5)) {
      lines.push(`  - ${node}`);
    }
    if (diff.nodesRemoved.length > 5) {
      lines.push(`  ... and ${diff.nodesRemoved.length - 5} more`);
    }
    lines.push("");
  }

  if (diff.edgesChanged > 0) {
    lines.push(`[CONNECTIONS] Changed: ${diff.edgesChanged}`);
    lines.push("");
  }

  // Summary
  lines.push("SUMMARY");
  lines.push("-------");
  lines.push(diff.description);
  lines.push("");

  return lines.join("\n");
}

export function formatVersionDetails(version: WorkflowVersion): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("VERSION DETAILS");
  lines.push("===============");
  lines.push("");

  lines.push(`Version: ${version.version}`);
  if (version.name) {
    lines.push(`Name: ${version.name}`);
  }
  if (version.description) {
    lines.push(`Description: ${version.description}`);
  }

  lines.push("");
  lines.push("STRUCTURE");
  lines.push("---------");
  lines.push(`  Nodes: ${version.nodeCount}`);
  lines.push(`  Edges: ${version.edgeCount}`);

  lines.push("");
  lines.push("METADATA");
  lines.push("--------");
  lines.push(`  Created: ${new Date(version.createdAt).toLocaleString()}`);
  lines.push(`  Updated: ${new Date(version.updatedAt).toLocaleString()}`);
  if (version.author) {
    lines.push(`  Author: ${version.author}`);
  }
  if (version.category) {
    lines.push(`  Category: ${version.category}`);
  }

  if (version.tags && version.tags.length > 0) {
    lines.push("");
    lines.push("TAGS");
    lines.push("----");
    for (const tag of version.tags) {
      lines.push(`  - ${tag}`);
    }
  }

  lines.push("");

  return lines.join("\n");
}

export function formatChangeLog(
  changelog: Array<{
    version: string;
    date: string;
    changes: string[];
    author?: string;
  }>
): string {
  if (changelog.length === 0) {
    return "No changelog available";
  }

  const lines: string[] = [];

  lines.push("");
  lines.push("CHANGELOG");
  lines.push("=========");
  lines.push("");

  for (const entry of changelog) {
    lines.push(`[Entry] Version ${entry.version}`);
    lines.push(`   Date: ${new Date(entry.date).toLocaleString()}`);
    if (entry.author) {
      lines.push(`   Author: ${entry.author}`);
    }

    lines.push("   Changes:");
    for (const change of entry.changes) {
      lines.push(`     - ${change}`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Format a table-style version list
 */
export function formatVersionTable(versions: WorkflowVersion[]): string {
  if (versions.length === 0) {
    return "No versions found";
  }

  const lines: string[] = [];

  lines.push("");
  lines.push("+----------+----------+--------+--------+--------+");
  lines.push("| Version  | Created  | Nodes  | Edges  | Author |");
  lines.push("+----------+----------+--------+--------+--------+");

  for (const version of versions) {
    const versionStr = version.version.padEnd(8);
    const createdStr = new Date(version.createdAt)
      .toLocaleString()
      .slice(0, 10)
      .padEnd(8);
    const nodesStr = String(version.nodeCount).padEnd(6);
    const edgesStr = String(version.edgeCount).padEnd(6);
    const authorStr = (version.author || "-").slice(0, 6).padEnd(6);

    lines.push(
      `| ${versionStr} | ${createdStr} | ${nodesStr} | ${edgesStr} | ${authorStr} |`
    );
  }

  lines.push("+----------+----------+--------+--------+--------+");
  lines.push("");

  return lines.join("\n");
}
