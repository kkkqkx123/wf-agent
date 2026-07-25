/**
 * Server Configuration Accessor
 * Provides convenient configuration access API.
 *
 * Extends the base ConfigAccessor from @wf-agent/runtime with
 * server-specific getter methods.
 */

import type { ServerConfig } from "./types.js";
import { ConfigValidator } from "../config-validator.js";
import { ConfigAccessor as BaseConfigAccessor } from "@wf-agent/runtime";

/**
 * Server Configuration Accessor
 * Wraps ServerConfig with convenient getter methods.
 * Extends the base ConfigAccessor from runtime with server-specific functionality.
 */
export class ServerConfigAccessor extends BaseConfigAccessor<ServerConfig> {
  constructor(config?: ServerConfig) {
    super(config);
  }

  /**
   * Get the underlying accessor for generic operations.
   */
  getAccessor(): BaseConfigAccessor<ServerConfig> {
    return this;
  }

  /**
   * Get storage configuration.
   */
  getStorageConfig() {
    return this.get().storage;
  }

  /**
   * Get SQLite storage configuration.
   */
  getSqliteStorageConfig() {
    return this.get().storage?.sqlite;
  }

  /**
   * Get storage base directory.
   */
  getStorageBaseDir(): string {
    const config = this.get();
    if (config.storage?.type === "sqlite" && config.storage.sqlite) {
      const dbPath = config.storage.sqlite.dbPath;
      return dbPath.substring(0, dbPath.lastIndexOf("/") + 1);
    }
    return "./storage";
  }

  /**
   * Get output configuration.
   */
  getOutputConfig() {
    return this.get().output;
  }

  /**
   * Get output directory.
   */
  getOutputDir(): string {
    return this.get().output?.dir || "./outputs";
  }

  /**
   * Get log file pattern.
   */
  getLogFilePattern(): string {
    return this.get().output?.logFilePattern || "server-{date}.log";
  }

  /**
   * Check if log terminal is enabled.
   */
  isLogTerminalEnabled(): boolean {
    return this.get().output?.enableLogTerminal ?? true;
  }

  /**
   * Check if SDK logs are enabled.
   */
  isSDKLogsEnabled(): boolean {
    return this.get().output?.enableSDKLogs ?? true;
  }

  /**
   * Get SDK log level.
   */
  getSDKLogLevel(): string {
    return this.get().output?.sdkLogLevel || "silent";
  }

  /**
   * Get presets configuration.
   */
  getPresetsConfig() {
    return this.get().presets;
  }

  /**
   * Get the full configuration object.
   */
  getFullConfig(): ServerConfig {
    return this.get();
  }

  /**
   * Get a specific configuration value by key.
   */
  getValue<K extends keyof ServerConfig>(key: K): ServerConfig[K] {
    return this.get()[key];
  }

  /**
   * Validate the current configuration.
   */
  validate(): { valid: boolean; errors: string[] } {
    return ConfigValidator.validate(this.get());
  }

  /**
   * Validate the current configuration and throw if invalid.
   */
  validateOrThrow(): void {
    ConfigValidator.validateOrThrow(this.get());
  }
}

/**
 * Global server configuration accessor instance.
 */
let globalServerConfigAccessor: ServerConfigAccessor | null = null;

/**
 * Get the global server configuration accessor instance.
 * @param config Optional configuration to initialize with
 * @returns Server configuration accessor instance
 */
export function getServerConfigAccessor(config?: ServerConfig): ServerConfigAccessor {
  if (!globalServerConfigAccessor && config) {
    globalServerConfigAccessor = new ServerConfigAccessor(config);
  }
  if (!globalServerConfigAccessor) {
    throw new Error("ServerConfigAccessor not initialized. Call initServerConfigAccessor first.");
  }
  return globalServerConfigAccessor;
}

/**
 * Initialize the global server configuration accessor.
 * @param config Configuration to initialize with
 */
export function initServerConfigAccessor(config: ServerConfig): void {
  globalServerConfigAccessor = new ServerConfigAccessor(config);
}

/**
 * Reset the global server configuration accessor.
 */
export function resetServerConfigAccessor(): void {
  globalServerConfigAccessor = null;
}
