/**
 * Output Formatter
 * Output formatting utilities
 */

// ============================================
// Formatter
// ============================================

/**
 * Output Formatter
 */
export class Formatter {
  private _colorEnabled: boolean;

  constructor(colorEnabled: boolean = true) {
    this._colorEnabled = colorEnabled;
  }

  // ============================================
  // Status messages
  // ============================================

  /**
   * Format success message
   */
  success(message: string): string {
    const prefix = this._colorEnabled ? "\x1b[32m✓\x1b[0m" : "✓";
    return `${prefix} ${message}`;
  }

  /**
   * Format failure message
   */
  fail(message: string): string {
    const prefix = this._colorEnabled ? "\x1b[31m✗\x1b[0m" : "✗";
    return `${prefix} ${message}`;
  }

  /**
   * Format info message
   */
  info(message: string): string {
    const prefix = this._colorEnabled ? "\x1b[34mℹ\x1b[0m" : "ℹ";
    return `${prefix} ${message}`;
  }

  /**
   * Format warning message
   */
  warning(message: string): string {
    const prefix = this._colorEnabled ? "\x1b[33m⚠\x1b[0m" : "⚠";
    return `${prefix} ${message}`;
  }

  /**
   * Format error message
   */
  error(message: string): string {
    const label = this._colorEnabled ? "\x1b[31mError:\x1b[0m" : "Error:";
    return `${label} ${message}`;
  }

  // ============================================
  // Status Formatting
  // ============================================

  /**
   * Format status (with color)
   */
  status(status: string): string {
    if (!this._colorEnabled) {
      return status;
    }

    const normalized = status.toLowerCase();
    switch (normalized) {
      case "running":
      case "active":
        return `\x1b[32m${status}\x1b[0m`;
      case "paused":
      case "suspended":
        return `\x1b[33m${status}\x1b[0m`;
      case "stopped":
      case "cancelled":
      case "failed":
        return `\x1b[31m${status}\x1b[0m`;
      case "completed":
      case "success":
        return `\x1b[32m\x1b[1m${status}\x1b[0m`;
      default:
        return `\x1b[90m${status || "unknown"}\x1b[0m`;
    }
  }

  // ============================================
  // Data Formatting
  // ============================================

  /**
   * Format JSON
   */
  json(data: unknown): string {
    return JSON.stringify(data, null, 2);
  }

  /**
   * Format key-value pair
   */
  keyValue(key: string, value: string): string {
    const k = this._colorEnabled ? `\x1b[36m${key}\x1b[0m` : key;
    return `${k}: ${value}`;
  }

  /**
   * Format multiple key-value pairs
   */
  keyValuePairs(pairs: Record<string, string>): string {
    return Object.entries(pairs)
      .map(([key, value]) => this.keyValue(key, value))
      .join("\n");
  }

  // ============================================
  // List Formatting
  // ============================================

  /**
   * Format bullet list
   */
  bulletList(items: string[]): string {
    const bullet = this._colorEnabled ? "\x1b[36m•\x1b[0m" : "•";
    return items.map(item => `${bullet} ${item}`).join("\n");
  }

  /**
   * Format numbered list
   */
  numberedList(items: string[]): string {
    return items.map((item, index) => `${index + 1}. ${item}`).join("\n");
  }

  // ============================================
  // Title Formatting
  // ============================================

  /**
   * Format section title
   */
  section(title: string): string {
    if (!this._colorEnabled) {
      return `\n${title}\n${"─".repeat(title.length)}`;
    }
    return `\n\x1b[1m\x1b[4m${title}\x1b[0m\n${"─".repeat(title.length)}`;
  }

  /**
   * Format subsection title
   */
  subsection(title: string): string {
    if (!this._colorEnabled) {
      return title;
    }
    return `\x1b[1m${title}\x1b[0m`;
  }

  // ============================================
  // Table Formatting
  // ============================================

  /**
   * Format table
   */
  table(headers: string[], rows: string[][]): string {
    // Calculate column width
    const widths = headers.map((header, index) => {
      const maxRowWidth = Math.max(...rows.map(row => this._getVisibleLength(row[index] || "")));
      return Math.max(header.length, maxRowWidth);
    });

    // Format the table headers
    const headerRow = headers
      .map((header, index) => {
        const padded = header.padEnd(widths[index] ?? header.length);
        return this._colorEnabled ? `\x1b[1m${padded}\x1b[0m` : padded;
      })
      .join(" | ");

    // Formatted Divider
    const separator = widths.map(w => "─".repeat(w ?? 0)).join("-+-");

    // Formatting data rows
    const dataRows = rows.map(row =>
      row
        .map((cell, index) => {
          const visibleLen = this._getVisibleLength(cell || "");
          const targetWidth = widths[index] ?? 0;
          const padding = targetWidth - visibleLen;
          return (cell || "") + " ".repeat(Math.max(0, padding));
        })
        .join(" | "),
    );

    return [headerRow, separator, ...dataRows].join("\n");
  }

  // ============================================
  // Object Formatting
  // ============================================

  /**
   * Format workflow
   */
  workflow(workflow: { id?: string; name?: string; status?: string }): string {
    const name = workflow.name || "Unnamed";
    const id = workflow.id || "N/A";
    const status = this.status(workflow.status || "unknown");

    if (!this._colorEnabled) {
      return `${name} (${id}) - ${workflow.status || "unknown"}`;
    }
    return `\x1b[36m${name}\x1b[0m (\x1b[90m${id}\x1b[0m) - ${status}`;
  }

  /**
   * Format thread
   */
  thread(thread: { id?: string; workflowId?: string; status?: string }): string {
    const id = thread.id || "N/A";
    const status = this.status(thread.status || "unknown");
    const workflowId = thread.workflowId || "N/A";

    if (!this._colorEnabled) {
      return `${id} - ${thread.status || "unknown"} - ${workflowId}`;
    }
    return `\x1b[36m${id}\x1b[0m - ${status} - \x1b[90m${workflowId}\x1b[0m`;
  }

  // ============================================
  // Private method
  // ============================================

  /**
   * Get visible length of string (excluding ANSI escape codes)
   */
  private _getVisibleLength(str: string): number {
    // eslint-disable-next-line no-control-regex
    return str.replace(/\x1b\[[0-9;]*m/g, "").length;
  }

  // ============================================
  // Setter
  // ============================================

  /**
   * Set whether color is enabled
   */
  setColorEnabled(enabled: boolean): void {
    this._colorEnabled = enabled;
  }
}

// ============================================
// Factory & Global Instance
// ============================================

let globalFormatter: Formatter | null = null;

/**
 * Create formatter
 */
export function createFormatter(colorEnabled: boolean = true): Formatter {
  return new Formatter(colorEnabled);
}

/**
 * Get global formatter
 */
export function getFormatter(): Formatter {
  if (!globalFormatter) {
    globalFormatter = new Formatter();
  }
  return globalFormatter;
}

/**
 * Initialize global formatter
 */
export function initializeFormatter(colorEnabled: boolean = true): Formatter {
  globalFormatter = new Formatter(colorEnabled);
  return globalFormatter;
}
