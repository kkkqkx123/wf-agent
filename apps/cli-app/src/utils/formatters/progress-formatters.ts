/**
 * Progress Tracking Formatters
 * Format progress metrics for CLI output
 */

import type { ProgressMetrics } from "../../adapters/progress-tracking-adapter.js";

export function formatProgressBar(metrics: ProgressMetrics): string {
  const barLength = 40;
  const filledLength = Math.round(
    (metrics.progressPercentage / 100) * barLength
  );
  const emptyLength = barLength - filledLength;

  const bar = "=".repeat(filledLength) + "-".repeat(emptyLength);
  const percentage = metrics.progressPercentage.toFixed(1);

  let statusText = "[Running]";
  if (metrics.status === "completed") {
    statusText = "[Completed]";
  } else if (metrics.status === "failed") {
    statusText = "[Failed]";
  } else if (metrics.status === "paused") {
    statusText = "[Paused]";
  }

  return `${statusText} [${bar}] ${percentage}%`;
}

export function formatProgressMetrics(metrics: ProgressMetrics): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("EXECUTION PROGRESS");
  lines.push("==================");
  lines.push("");

  lines.push(`ID: ${metrics.executionId}`);
  lines.push(`Status: ${formatStatus(metrics.status)}`);
  lines.push("");

  lines.push(formatProgressBar(metrics));
  lines.push(
    `${metrics.iteration} / ${metrics.totalIterations} iterations`
  );
  lines.push("");

  lines.push("TIMING");
  lines.push("------");
  lines.push(`  Elapsed: ${formatDuration(metrics.elapsedTime)}`);

  if (metrics.status === "running" && metrics.estimatedRemainingTime > 0) {
    lines.push(`  Remaining: ${formatDuration(metrics.estimatedRemainingTime)}`);
    lines.push(
      `  Total Est.: ${formatDuration(metrics.estimatedTotalTime)}`
    );
  }

  if (metrics.estimatedCompletionTime) {
    lines.push(
      `  Est. Complete: ${metrics.estimatedCompletionTime.toLocaleTimeString()}`
    );
  }

  lines.push("");

  lines.push("PERFORMANCE");
  lines.push("-----------");
  lines.push(
    `  Iterations/sec: ${metrics.iterationsPerSecond.toFixed(2)}`
  );
  lines.push(`  Tool Calls/sec: ${metrics.toolCallsPerSecond.toFixed(2)}`);
  lines.push(`  Confidence: ${(metrics.confidence * 100).toFixed(0)}%`);
  lines.push("");

  return lines.join("\n");
}

export function formatTimeEstimate(metrics: ProgressMetrics): string {
  if (metrics.status !== "running") {
    return `Status: ${formatStatus(metrics.status)}`;
  }

  const lines: string[] = [];

  if (metrics.estimatedRemainingTime > 0) {
    lines.push("[TIME REMAINING]");
    lines.push(formatDuration(metrics.estimatedRemainingTime));
  }

  if (metrics.estimatedCompletionTime) {
    lines.push("[ESTIMATED COMPLETION]");
    lines.push(metrics.estimatedCompletionTime.toLocaleTimeString());
  }

  lines.push(`Confidence: ${(metrics.confidence * 100).toFixed(0)}%`);

  return lines.join("\n");
}

export function formatProgressTable(metrics: ProgressMetrics): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("+---------------------------------------------------------+");
  lines.push("|                    EXECUTION PROGRESS                   |");
  lines.push("+---------------------------------------------------------+");

  // Status row
  const statusLine = `| Status: ${formatStatus(metrics.status).padEnd(50)} |`;
  lines.push(statusLine);

  // Progress bar row
  const barLength = 40;
  const filledLength = Math.round(
    (metrics.progressPercentage / 100) * barLength
  );
  const emptyLength = barLength - filledLength;
  const bar = "=".repeat(filledLength) + "-".repeat(emptyLength);
  lines.push(
    `| Progress: [${bar}] ${metrics.progressPercentage.toFixed(1)}% |`
  );

  // Iterations row
  const iterLine = `| Iterations: ${metrics.iteration}/${metrics.totalIterations}`.padEnd(57) + "|";
  lines.push(iterLine);

  lines.push("+---------------------------------------------------------+");

  // Timing section
  const elapsedLine = `| Elapsed:  ${formatDuration(metrics.elapsedTime)}`.padEnd(57) + "|";
  lines.push(elapsedLine);

  if (metrics.status === "running" && metrics.estimatedRemainingTime > 0) {
    const remainingLine = `| Remaining: ${formatDuration(metrics.estimatedRemainingTime)}`.padEnd(57) + "|";
    lines.push(remainingLine);

    const estLine = `| Total Est.: ${formatDuration(metrics.estimatedTotalTime)}`.padEnd(57) + "|";
    lines.push(estLine);
  }

  if (metrics.estimatedCompletionTime) {
    const completeLine = `| Completion: ${metrics.estimatedCompletionTime.toLocaleTimeString()}`.padEnd(57) + "|";
    lines.push(completeLine);
  }

  lines.push("+---------------------------------------------------------+");

  // Performance section
  const iterSecLine = `| Iter/sec: ${metrics.iterationsPerSecond.toFixed(2)}`.padEnd(57) + "|";
  lines.push(iterSecLine);

  const toolSecLine = `| Tools/sec: ${metrics.toolCallsPerSecond.toFixed(2)}`.padEnd(57) + "|";
  lines.push(toolSecLine);

  const confLine = `| Confidence: ${(metrics.confidence * 100).toFixed(0)}%`.padEnd(57) + "|";
  lines.push(confLine);

  lines.push("+---------------------------------------------------------+");
  lines.push("");

  return lines.join("\n");
}

/**
 * Helper functions
 */
function formatStatus(status: string): string {
  switch (status) {
    case "completed":
      return "[OK] Completed";
    case "failed":
      return "[ERROR] Failed";
    case "paused":
      return "[PAUSED]";
    case "running":
    default:
      return "[RUNNING]";
  }
}

function formatDuration(milliseconds: number): string {
  if (milliseconds < 0) return "Unknown";

  const totalSeconds = Math.floor(milliseconds / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m ${seconds}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}
