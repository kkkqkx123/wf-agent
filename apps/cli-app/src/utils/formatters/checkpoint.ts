/**
 * Checkpoint formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { Checkpoint } from "@wf-agent/types";

// Type alias for checkpoint with createdAt field
type CheckpointWithMetadata = Checkpoint & {
  createdAt?: string | number;
};

export function formatCheckpoint(checkpoint: CheckpointWithMetadata, options?: { verbose?: boolean }): string {
  return formatWith(checkpoint, options, () => {
    return `${checkpoint.id || "N/A"} - ${checkpoint.executionId || "N/A"} - ${checkpoint.createdAt || "N/A"}`;
  });
}

export function formatCheckpointList(checkpoints: CheckpointWithMetadata[], options?: { table?: boolean }): string {
  if (checkpoints.length === 0) {
    return emptyMsg("checkpoints");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Checkpoint ID", "Execution ID", "Creation time"];
    const rows = checkpoints.map(c => [
      shortId(c.id),
      shortId(c.executionId),
      String(c.createdAt || "N/A"),
    ]);
    return formatter.table(headers, rows);
  }

  return checkpoints.map(c => formatCheckpoint(c)).join("\n");
}