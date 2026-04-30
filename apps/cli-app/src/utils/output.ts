/**
 * CLI Output Manager
 * Unified management of stdout, stderr, log three output streams
 */

import type { Writable } from "stream";
import * as fs from "fs";
import * as path from "path";
import { Formatter, getFormatter } from "./formatter.js";

// ============================================
// Types
// ============================================

/**
 * Output configuration
 */
export interface OutputConfig {
  /** Log file path */
  logFile?: string;
  /** Whether to enable colors */
  color?: boolean;
  /** Is it in detail mode? */
  verbose?: boolean;
  /** Is it in debug mode? */
  debug?: boolean;
  /** Output directory (from config) */
  outputDir?: string;
  /** Log file pattern (from config) */
  logFilePattern?: string;
  /** Whether to enable log terminal output */
  enableLogTerminal?: boolean;
  /** Whether to enable SDK logs */
  enableSDKLogs?: boolean;
  /** SDK log level */
  sdkLogLevel?: string;
}

// ============================================
// CLI Output Manager
// ============================================

/**
 * CLI output manager
 * Unified management of stdout, stderr, log three output streams
 */
export class CLIOutput {
  private readonly _stdout: Writable;
  private readonly _stderr: Writable;
  private readonly _logStream: fs.WriteStream | null;
  private readonly _colorEnabled: boolean;
  private readonly _verbose: boolean;
  private readonly _debug: boolean;
  private readonly _logFile: string;
  private readonly _config: OutputConfig;

  constructor(config: OutputConfig = {}) {
    this._config = config;
    this._stdout = process.stdout;
    this._stderr = process.stderr;
    this._colorEnabled = config.color ?? this._supportsColor();
    this._verbose = config.verbose ?? false;
    this._debug = config.debug ?? false;

    // Initialize the log file
    this._logFile = config.logFile || this._getDefaultLogPath();
    this._logStream = fs.createWriteStream(this._logFile, { flags: "a" });
  }

  // ============================================
  // User output (stdout)
  // ============================================

  /**
   * Output to stdout (with newline)
   */
  output(content: string): void {
    this._stdout.write(content + "\n");
  }

  /**
   * Output to stdout (without newline)
   */
  write(content: string): void {
    this._stdout.write(content);
  }

  /**
   * Stream output (alias, without newline)
   */
  stream(content: string): void {
    this._stdout.write(content);
  }

  /**
   * Output empty line
   */
  newLine(): void {
    this._stdout.write("\n");
  }

  /**
   * Output success message
   */
  success(message: string): void {
    const prefix = this._colorEnabled ? "\x1b[32m✓\x1b[0m" : "✓";
    this.output(`${prefix} ${message}`);
  }

  /**
   * Output info message
   */
  info(message: string): void {
    const prefix = this._colorEnabled ? "\x1b[34mℹ\x1b[0m" : "ℹ";
    this.output(`${prefix} ${message}`);
  }

  /**
   * Output warning message (to stdout, for user-visible warnings)
   */
  warn(message: string): void {
    const prefix = this._colorEnabled ? "\x1b[33m⚠\x1b[0m" : "⚠";
    this.output(`${prefix} ${message}`);
  }

  // ============================================
  // Error output (stderr)
  // ============================================

  /**
   * Output to stderr
   */
  error(content: string): void {
    this._stderr.write(content + "\n");
  }

  /**
   * Output fail message
   */
  fail(message: string): void {
    const prefix = this._colorEnabled ? "\x1b[31m✗\x1b[0m" : "✗";
    this.error(`${prefix} ${message}`);
  }

  /**
   * Output error message (with colored label)
   */
  errorWithLabel(label: string, message: string): void {
    const coloredLabel = this._colorEnabled ? `\x1b[31m${label}\x1b[0m` : label;
    this.error(`${coloredLabel}: ${message}`);
  }

  // ============================================
  // Log output
  // ============================================

  /**
   * Write to log file
   */
  log(level: string, message: string, context?: Record<string, unknown>): void {
    if (!this._logStream) return;

    const timestamp = new Date().toISOString();
    const contextStr = context ? ` ${JSON.stringify(context)}` : "";
    this._logStream.write(`[${timestamp}] [${level.toUpperCase()}] ${message}${contextStr}\n`);
  }

  /**
   * Debug log
   */
  debugLog(message: string, context?: Record<string, unknown>): void {
    if (this._debug) {
      this.log("debug", message, context);
    }
  }

  /**
   * Verbose log
   */
  verboseLog(message: string, context?: Record<string, unknown>): void {
    if (this._verbose || this._debug) {
      this.log("info", message, context);
    }
  }

  /**
   * Info log
   */
  infoLog(message: string, context?: Record<string, unknown>): void {
    this.log("info", message, context);
  }

  /**
   * Warn log
   */
  warnLog(message: string, context?: Record<string, unknown>): void {
    this.log("warn", message, context);
  }

  /**
   * Error log
   */
  errorLog(message: string, context?: Record<string, unknown>): void {
    this.log("error", message, context);
  }

  // ============================================
  // Getter
  // ============================================

  /** Get the path of the log file. */
  get logFile(): string {
    return this._logFile;
  }

  /** Obtain the log stream (for use by the Logger) */
  get logStream(): fs.WriteStream | null {
    return this._logStream;
  }

  /** Whether to enable colors */
  get colorEnabled(): boolean {
    return this._colorEnabled;
  }

  /** Is it in detail mode? */
  get verbose(): boolean {
    return this._verbose;
  }

  /** Is it in debug mode? */
  get debug(): boolean {
    return this._debug;
  }

  /** Get the formatter */
  get formatter(): Formatter {
    return getFormatter();
  }

  // ============================================
  // Formatting Output Method (Compatible with Old APIs)
  // ============================================

  /**
   * Output JSON data
   */
  json(data: unknown): void {
    this.output(getFormatter().json(data));
  }

  /**
   * Output table
   */
  table(headers: string[], rows: string[][]): void {
    this.output(getFormatter().table(headers, rows));
  }

  /**
   * Output bullet list
   */
  bulletList(items: string[]): void {
    this.output(getFormatter().bulletList(items));
  }

  /**
   * Output numbered list
   */
  numberedList(items: string[]): void {
    this.output(getFormatter().numberedList(items));
  }

  /**
   * Output section title
   */
  section(title: string): void {
    this.output(getFormatter().section(title));
  }

  /**
   * Output subsection title
   */
  subsection(title: string): void {
    this.output(getFormatter().subsection(title));
  }

  /**
   * Output key-value pair
   */
  keyValue(key: string, value: string): void {
    this.output(getFormatter().keyValue(key, value));
  }

  /**
   * Output multiple key-value pairs
   */
  keyValuePairs(pairs: Record<string, string>): void {
    this.output(getFormatter().keyValuePairs(pairs));
  }

  /**
   * Format workflow
   */
  workflow(workflow: { id?: string; name?: string; status?: string }): string {
    return getFormatter().workflow(workflow);
  }

  /**
   * Format workflow execution
   */
  workflowExecution(execution: { id?: string; workflowId?: string; status?: string }): string {
    return getFormatter().workflowExecution(execution);
  }

  /**
   * Format status
   */
  status(status: string): string {
    return getFormatter().status(status);
  }

  // ============================================
  // Lifecycle
  // ============================================

  /**
   * Flush and close all streams
   */
  async close(): Promise<void> {
    return new Promise(resolve => {
      if (this._logStream) {
        this._logStream.end(() => resolve());
      } else {
        resolve();
      }
    });
  }

  /**
   * Flush log stream
   */
  flush(): Promise<void> {
    return new Promise(resolve => {
      if (this._logStream && this._logStream.writable) {
        this._logStream.write("", () => resolve());
      } else {
        resolve();
      }
    });
  }

  /**
   * Ensure all output streams are drained
   * Used for safe exit in headless mode
   */
  async ensureDrained(): Promise<void> {
    const drains: Promise<void>[] = [];

    // Wait for stdout
    if (this._stdout.writable) {
      drains.push(
        new Promise(resolve => {
          if ((this._stdout as any).writableNeedDrain) {
            this._stdout.once("drain", resolve);
          } else {
            resolve();
          }
        }),
      );
    }

    // Wait for stderr
    if (this._stderr.writable) {
      drains.push(
        new Promise(resolve => {
          if ((this._stderr as any).writableNeedDrain) {
            this._stderr.once("drain", resolve);
          } else {
            resolve();
          }
        }),
      );
    }

    // Wait for log stream
    if (this._logStream && this._logStream.writable) {
      drains.push(
        new Promise(resolve => {
          this._logStream!.once("finish", resolve);
          this._logStream!.end();
        }),
      );
    }

    await Promise.all(drains);
  }

  /**
   * Output structured data (JSON mode)
   */
  structuredOutput(data: unknown): void {
    this._stdout.write(JSON.stringify(data) + "\n");
  }

  /**
   * Output result (auto-select format based on mode)
   */
  result(data: unknown, options?: { message?: string; success?: boolean }): void {
    const { message, success = true } = options || {};

    const isJsonMode =
      process.env["CLI_OUTPUT_FORMAT"] === "json" ||
      process.env["CLI_MODE"] === "headless" ||
      process.env["HEADLESS"] === "true";

    if (isJsonMode) {
      this.structuredOutput({
        success,
        data,
        message,
        timestamp: new Date().toISOString(),
      });
    } else {
      if (success) {
        this.success(message || "Operation completed");
      } else {
        this.fail(message || "Operation failed");
      }
      if (data) {
        this.json(data);
      }
    }
  }

  /**
   * Output error result (auto-select format based on mode)
   */
  errorResult(error: Error | string, code?: string): void {
    const errorMessage = error instanceof Error ? error.message : error;

    const isJsonMode =
      process.env["CLI_OUTPUT_FORMAT"] === "json" ||
      process.env["CLI_MODE"] === "headless" ||
      process.env["HEADLESS"] === "true";

    if (isJsonMode) {
      this.structuredOutput({
        success: false,
        error: {
          message: errorMessage,
          code,
          timestamp: new Date().toISOString(),
        },
      });
    } else {
      this.fail(errorMessage);
    }
  }

  // ============================================
  // Private method
  // ============================================

  private _supportsColor(): boolean {
    return process.stdout.isTTY === true && process.env["NO_COLOR"] === undefined;
  }

  private _getDefaultLogPath(): string {
    const outputDir =
      this._config.outputDir ??
      (process.env["TEST_MODE"] && process.env["LOG_DIR"]
        ? process.env["LOG_DIR"]
        : path.join(process.cwd(), "logs"));

    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true });
    }

    const pattern = this._config.logFilePattern || "cli-app-{date}.log";
    const date = new Date().toISOString().split("T")[0] || "";
    const logFileName = pattern.replace("{date}", date);

    return path.join(outputDir, logFileName);
  }
}

// ============================================
// Global Instance
// ============================================

let globalOutput: CLIOutput | null = null;

/**
 * Initialize global output manager
 */
export function initializeOutput(config: OutputConfig = {}): CLIOutput {
  globalOutput = new CLIOutput(config);
  return globalOutput;
}

/**
 * Get global output manager
 */
export function getOutput(): CLIOutput {
  if (!globalOutput) {
    return initializeOutput();
  }
  return globalOutput;
}

/**
 * Reset global output manager
 */
export function resetOutput(): void {
  if (globalOutput) {
    globalOutput.close().catch(() => {});
    globalOutput = null;
  }
}
