/**
 * Diff Engine - Myers diff algorithm implementation
 *
 * Provides efficient text diffing capabilities for file checkpoint comparisons.
 * Based on the Myers O(ND) algorithm with optimizations.
 *
 * Reference: "An O(ND) Difference Algorithm and Its Variations" by Eugene W. Myers
 */

/**
 * Single diff operation
 */
export interface DiffOp {
  /** Operation type */
  type: "equal" | "delete" | "insert";
  /** Text content */
  value: string;
  /** Line number in old content (for delete/equal) */
  oldLine?: number;
  /** Line number in new content (for insert/equal) */
  newLine?: number;
}

/**
 * Diff result with metadata
 */
export interface DiffResult {
  /** Diff operations */
  ops: DiffOp[];
  /** Number of equal lines */
  equalCount: number;
  /** Number of deleted lines */
  deleteCount: number;
  /** Number of inserted lines */
  insertCount: number;
  /** Whether there are any changes */
  hasChanges: boolean;
}

/**
 * Diff engine configuration
 */
export interface DiffEngineConfig {
  /** Number of context lines for unified diff (default: 3) */
  contextLines?: number;
  /** Whether to trim lines before comparing (default: false) */
  trimLines?: boolean;
  /** Whether to ignore blank line differences (default: false) */
  ignoreBlankLines?: boolean;
}

/**
 * Unified diff hunk
 */
export interface DiffHunk {
  /** Start line in old content */
  oldStart: number;
  /** Number of lines in old content */
  oldCount: number;
  /** Start line in new content */
  newStart: number;
  /** Number of lines in new content */
  newCount: number;
  /** Lines in this hunk */
  lines: Array<{ type: "context" | "delete" | "insert"; value: string }>;
}

/**
 * Diff Engine
 *
 * Implements Myers diff algorithm for efficient text comparison.
 */
export class DiffEngine {
  private readonly config: Required<DiffEngineConfig>;

  constructor(config?: DiffEngineConfig) {
    this.config = {
      contextLines: config?.contextLines ?? 3,
      trimLines: config?.trimLines ?? false,
      ignoreBlankLines: config?.ignoreBlankLines ?? false,
    };
  }

  /**
   * Compute diff between two strings
   */
  diff(oldContent: string, newContent: string): DiffResult {
    const oldLines = this.preprocessLines(oldContent);
    const newLines = this.preprocessLines(newContent);

    const ops = this.myersDiff(oldLines, newLines);

    let equalCount = 0;
    let deleteCount = 0;
    let insertCount = 0;

    for (const op of ops) {
      switch (op.type) {
        case "equal":
          equalCount++;
          break;
        case "delete":
          deleteCount++;
          break;
        case "insert":
          insertCount++;
          break;
      }
    }

    return {
      ops,
      equalCount,
      deleteCount,
      insertCount,
      hasChanges: deleteCount > 0 || insertCount > 0,
    };
  }

  /**
   * Generate unified diff format
   */
  unifiedDiff(
    oldContent: string,
    newContent: string,
    oldPath?: string,
    newPath?: string,
  ): string {
    const result = this.diff(oldContent, newContent);

    if (!result.hasChanges) {
      return "";
    }

    const hunks = this.groupIntoHunks(result.ops);
    const lines: string[] = [];

    // Header
    if (oldPath && newPath) {
      lines.push(`--- ${oldPath}`);
      lines.push(`+++ ${newPath}`);
    }

    // Hunks
    for (const hunk of hunks) {
      lines.push(
        `@@ -${hunk.oldStart},${hunk.oldCount} +${hunk.newStart},${hunk.newCount} @@`,
      );

      for (const line of hunk.lines) {
        switch (line.type) {
          case "context":
            lines.push(` ${line.value}`);
            break;
          case "delete":
            lines.push(`-${line.value}`);
            break;
          case "insert":
            lines.push(`+${line.value}`);
            break;
        }
      }
    }

    return lines.join("\n");
  }

  /**
   * Get diff statistics
   */
  getStats(oldContent: string, newContent: string): {
    addedLines: number;
    removedLines: number;
    changedLines: number;
    similarity: number;
  } {
    const result = this.diff(oldContent, newContent);

    const totalLines = result.equalCount + result.deleteCount + result.insertCount;
    const similarity = totalLines > 0 ? result.equalCount / totalLines : 1;

    return {
      addedLines: result.insertCount,
      removedLines: result.deleteCount,
      changedLines: result.deleteCount + result.insertCount,
      similarity,
    };
  }

  /**
   * Preprocess content into lines
   */
  private preprocessLines(content: string): string[] {
    let lines = content.split("\n");

    if (this.config.trimLines) {
      lines = lines.map((line) => line.trim());
    }

    if (this.config.ignoreBlankLines) {
      lines = lines.filter((line) => line.length > 0);
    }

    // Remove trailing empty line if present
    if (lines.length > 0 && lines[lines.length - 1] === "") {
      lines.pop();
    }

    return lines;
  }

  /**
   * Myers O(ND) diff algorithm
   *
   * Finds the shortest edit script that transforms oldLines into newLines.
   */
  private myersDiff(oldLines: string[], newLines: string[]): DiffOp[] {
    const n = oldLines.length;
    const m = newLines.length;

    if (n === 0 && m === 0) {
      return [];
    }

    if (n === 0) {
      return newLines.map((line, i) => ({
        type: "insert" as const,
        value: line,
        newLine: i + 1,
      }));
    }

    if (m === 0) {
      return oldLines.map((line, i) => ({
        type: "delete" as const,
        value: line,
        oldLine: i + 1,
      }));
    }

    // Find edit script using Myers algorithm
    const editScript = this.findEditScript(oldLines, newLines);

    // Convert edit script to diff ops
    return this.editScriptToDiffOps(editScript, oldLines, newLines);
  }

  /**
   * Find edit script using Myers algorithm
   *
   * Returns array of [oldIndex, newIndex] pairs representing the LCS (Longest Common Subsequence).
   */
  private findEditScript(
    oldLines: string[],
    newLines: string[],
  ): Array<[number, number]> {
    const n = oldLines.length;
    const m = newLines.length;
    const max = n + m;

    // V array stores the furthest reaching D-path for each diagonal k
    const v: Map<number, number> = new Map();
    v.set(1, 0);

    // Trace array for backtracking
    const trace: Map<number, number>[] = [];

    for (let d = 0; d <= max; d++) {
      const currentV = new Map<number, number>();
      trace.push(currentV);

      for (let k = -d; k <= d; k += 2) {
        // Decide whether to go down or right
        let x: number;
        if (k === -d || (k !== d && (v.get(k - 1) ?? 0) < (v.get(k + 1) ?? 0))) {
          x = v.get(k + 1) ?? 0;
        } else {
          x = (v.get(k - 1) ?? 0) + 1;
        }

        let y = x - k;

        // Extend diagonal
        while (x < n && y < m && oldLines[x] === newLines[y]) {
          x++;
          y++;
        }

        v.set(k, x);
        currentV.set(k, x);

        // Check if we've reached the end
        if (x >= n && y >= m) {
          // Backtrack to find LCS
          return this.backtrack(trace, d, n, m);
        }
      }
    }

    return [];
  }

  /**
   * Backtrack through trace to find LCS
   */
  private backtrack(
    trace: Map<number, number>[],
    d: number,
    n: number,
    m: number,
  ): Array<[number, number]> {
    const lcs: Array<[number, number]> = [];
    let x = n;
    let y = m;

    for (let currentD = d; currentD > 0; currentD--) {
      const v = trace[currentD]!;
      const k = x - y;

      const prevK =
        k === -currentD ||
        (k !== currentD && (v.get(k - 1) ?? 0) < (v.get(k + 1) ?? 0))
          ? k + 1
          : k - 1;

      const prevX = v.get(prevK) ?? 0;
      const prevY = prevX - prevK;

      // Add diagonal moves to LCS
      while (x > prevX && y > prevY) {
        x--;
        y--;
        lcs.unshift([x, y]);
      }

      x = prevX;
      y = prevY;
    }

    // Add remaining diagonal moves from start
    while (x > 0 && y > 0) {
      x--;
      y--;
      lcs.unshift([x, y]);
    }

    return lcs;
  }

  /**
   * Convert LCS edit script to diff operations
   */
  private editScriptToDiffOps(
    lcs: Array<[number, number]>,
    oldLines: string[],
    newLines: string[],
  ): DiffOp[] {
    const ops: DiffOp[] = [];
    let oldIndex = 0;
    let newIndex = 0;
    let lcsIndex = 0;

    while (oldIndex < oldLines.length || newIndex < newLines.length) {
      if (lcsIndex < lcs.length) {
        const [lcsOld, lcsNew] = lcs[lcsIndex]!;

        // Delete lines before LCS point
        while (oldIndex < lcsOld) {
          ops.push({
            type: "delete",
            value: oldLines[oldIndex]!,
            oldLine: oldIndex + 1,
          });
          oldIndex++;
        }

        // Insert lines before LCS point
        while (newIndex < lcsNew) {
          ops.push({
            type: "insert",
            value: newLines[newIndex]!,
            newLine: newIndex + 1,
          });
          newIndex++;
        }

        // Equal line (LCS point)
        ops.push({
          type: "equal",
          value: oldLines[oldIndex]!,
          oldLine: oldIndex + 1,
          newLine: newIndex + 1,
        });
        oldIndex++;
        newIndex++;
        lcsIndex++;
      } else {
        // Remaining deletes
        while (oldIndex < oldLines.length) {
          ops.push({
            type: "delete",
            value: oldLines[oldIndex]!,
            oldLine: oldIndex + 1,
          });
          oldIndex++;
        }

        // Remaining inserts
        while (newIndex < newLines.length) {
          ops.push({
            type: "insert",
            value: newLines[newIndex]!,
            newLine: newIndex + 1,
          });
          newIndex++;
        }
      }
    }

    return ops;
  }

  /**
   * Group diff ops into hunks for unified diff format
   */
  private groupIntoHunks(ops: DiffOp[]): DiffHunk[] {
    const hunks: DiffHunk[] = [];
    const contextLines = this.config.contextLines;

    // Find change boundaries
    const changeIndices: number[] = [];
    for (let i = 0; i < ops.length; i++) {
      if (ops[i]!.type !== "equal") {
        changeIndices.push(i);
      }
    }

    if (changeIndices.length === 0) {
      return hunks;
    }

    // Group changes into hunks
    let hunkStart = Math.max(0, changeIndices[0]! - contextLines);
    let hunkEnd = Math.min(
      ops.length - 1,
      changeIndices[changeIndices.length - 1]! + contextLines,
    );

    // Check if changes should be merged
    for (let i = 1; i < changeIndices.length; i++) {
      const prevChange = changeIndices[i - 1]!;
      const currChange = changeIndices[i]!;

      if (currChange - prevChange <= 2 * contextLines + 1) {
        // Merge hunks
        hunkEnd = Math.min(ops.length - 1, currChange + contextLines);
      } else {
        // Create hunk
        hunks.push(this.createHunk(ops, hunkStart, hunkEnd));

        // Start new hunk
        hunkStart = Math.max(0, currChange - contextLines);
        hunkEnd = Math.min(ops.length - 1, currChange + contextLines);
      }
    }

    // Add final hunk
    hunks.push(this.createHunk(ops, hunkStart, hunkEnd));

    return hunks;
  }

  /**
   * Create a hunk from a range of diff ops
   */
  private createHunk(ops: DiffOp[], start: number, end: number): DiffHunk {
    const lines: Array<{ type: "context" | "delete" | "insert"; value: string }> =
      [];

    let oldStart = 0;
    let newStart = 0;
    let oldCount = 0;
    let newCount = 0;

    // Find start positions
    for (let i = 0; i <= start; i++) {
      const op = ops[i]!;
      if (op.type === "equal" || op.type === "delete") {
        oldStart = op.oldLine ?? oldStart;
      }
      if (op.type === "equal" || op.type === "insert") {
        newStart = op.newLine ?? newStart;
      }
    }

    // Count lines
    for (let i = start; i <= end; i++) {
      const op = ops[i]!;
      switch (op.type) {
        case "equal":
          lines.push({ type: "context", value: op.value });
          oldCount++;
          newCount++;
          break;
        case "delete":
          lines.push({ type: "delete", value: op.value });
          oldCount++;
          break;
        case "insert":
          lines.push({ type: "insert", value: op.value });
          newCount++;
          break;
      }
    }

    return {
      oldStart,
      oldCount,
      newStart,
      newCount,
      lines,
    };
  }
}
