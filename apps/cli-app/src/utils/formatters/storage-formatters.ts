/**
 * Storage Diagnostics Formatters
 * Format storage diagnostic data for CLI output
 */

import type {
  StorageDiagnosticsReport,
  StorageItemCounts,
  StorageAdapterHealth,
} from "@wf-agent/sdk/api";

export function formatStorageDiagnosticsReport(report: StorageDiagnosticsReport): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("📊 Storage Diagnostics Report");
  lines.push("");

  const statusIcon =
    report.overallStatus === "healthy"
      ? "✅"
      : report.overallStatus === "degraded"
        ? "⚠️"
        : "❌";
  lines.push(`Overall Status: ${statusIcon} ${report.overallStatus.toUpperCase()}`);
  lines.push("");

  lines.push("Adapter Health:");
  for (const adapter of report.adapterHealth) {
    const configIcon = adapter.configured ? "✓" : "✗";
    const statusIcon2 =
      adapter.status === "healthy" ? "🟢" : adapter.status === "error" ? "🔴" : "⚪";

    lines.push(
      `  ${statusIcon2} ${configIcon} ${adapter.name.padEnd(20)} - ${adapter.status}`
    );

    if (adapter.error) {
      lines.push(`      Error: ${adapter.error}`);
    }
  }
  lines.push("");

  if (report.itemCounts) {
    lines.push("Item Counts:");
    const counts = report.itemCounts;
    lines.push(`  Workflows:   ${counts.workflows || 0}`);
    lines.push(`  Executions:  ${counts.executions || 0}`);
    lines.push(`  Graphs:      ${counts.graphs || 0}`);
    lines.push(`  Checkpoints: ${(counts as any).checkpoints || 0}`);

    if (counts.tasks) {
      lines.push(`  Tasks:       ${counts.tasks.total}`);
      lines.push(
        `    - Queued: ${counts.tasks.queued}, Running: ${counts.tasks.running}, ` +
          `Completed: ${counts.tasks.completed}, Failed: ${counts.tasks.failed}`
      );
    }
    lines.push("");
  }

  if ((report as any).warnings && (report as any).warnings.length > 0) {
    lines.push("Warnings:");
    for (const warning of (report as any).warnings) {
      lines.push(`  ⚠️  ${warning}`);
    }
    lines.push("");
  }

  lines.push(`Generated at: ${new Date().toISOString()}`);
  lines.push("");

  return lines.join("\n");
}

export function formatStorageHealth(health: {
  status: string;
  adaptersConfigured: number;
  adapterDetails: StorageAdapterHealth[];
}): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("🏥 Storage Health Status");
  lines.push("");

  const statusEmoji = health.status === "healthy" ? "✅" : "⚠️";
  lines.push(`${statusEmoji} Status: ${health.status.toUpperCase()}`);
  lines.push(`📦 Adapters: ${health.adaptersConfigured} configured`);
  lines.push("");

  for (const adapter of health.adapterDetails) {
    const icon = adapter.configured ? "✓" : "✗";
    const status =
      adapter.status === "healthy"
        ? "healthy"
        : adapter.status === "error"
          ? "error"
          : "not configured";
    lines.push(`  ${icon} ${adapter.name.padEnd(15)} - ${status}`);
  }
  lines.push("");

  return lines.join("\n");
}

export function formatStorageItemCounts(counts: StorageItemCounts): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("Storage Statistics:");
  lines.push(`  Workflows:   ${counts.workflows || 0}`);
  lines.push(`  Executions:  ${counts.executions || 0}`);
  lines.push(`  Graphs:      ${counts.graphs || 0}`);
  lines.push(`  Checkpoints: ${(counts as any).checkpoints || 0}`);

  if (counts.tasks) {
    lines.push(`  Tasks:       ${counts.tasks.total}`);
    lines.push(
      `    - Queued: ${counts.tasks.queued}, Running: ${counts.tasks.running}, ` +
        `Completed: ${counts.tasks.completed}`
    );
  }
  lines.push("");

  return lines.join("\n");
}
