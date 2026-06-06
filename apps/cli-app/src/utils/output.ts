/**
 * CLI Output Manager
 * Unified management of stdout, stderr, log three output streams
 */

import type { Writable } from "stream";
import * as fs from "fs";
import * as path from "path";

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
  private _logStream: fs.WriteStream | null;
  private _colorEnabled: boolean;
  private _verbose: boolean;
  private _debug: boolean;
  private _logFile: string;
  private _config: OutputConfig;
  private _closed: boolean = false;

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

  /**
   * Reconfigure the output manager in-place.
   * Ensures that module-level captures (e.g. in command files) remain valid
   * after preAction configures the instance with proper settings.
   */
  reconfigure(config: OutputConfig): void {
    this._config = { ...this._config, ...config };
    if (config.color !== undefined) {
      this._colorEnabled = config.color;
    }
    if (config.verbose !== undefined) {
      this._verbose = config.verbose;
    }
    if (config.debug !== undefined) {
      this._debug = config.debug;
    }

    // If log file path changed, close old stream and open new one
    const newLogFile = config.logFile || this._getDefaultLogPath();
    if (newLogFile !== this._logFile) {
      if (this._logStream && !this._closed) {
        this._logStream.end();
      }
      this._logFile = newLogFile;
      this._logStream = fs.createWriteStream(this._logFile, { flags: "a" });
    }
  }

  /**
   * Replace the log file stream (used by the logger system to install its rotating stream).
   * This ensures only one stream writes to the log file, avoiding dual-write conflicts.
   * The old stream (if any) is ended gracefully.
   */
  setLogStream(stream: fs.WriteStream | null): void {
    if (this._logStream && this._logStream !== stream && !this._closed) {
      this._logStream.end();
    }
    this._logStream = stream;
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

  // ============================================
  // Lifecycle
  // ============================================

  /**
   * Flush and close all streams
   */
  async close(): Promise<void> {
    if (this._closed) return;
    this._closed = true;

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
          if ((this._stdout as unknown as { writableNeedDrain?: boolean }).writableNeedDrain) {
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
          if ((this._stderr as unknown as { writableNeedDrain?: boolean }).writableNeedDrain) {
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
export async function resetOutput(): Promise<void> {
  if (globalOutput) {
    await globalOutput.close();
    globalOutput = null;
  }
}
