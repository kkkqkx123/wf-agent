/**
 * Plugin Types - Core type definitions for the plugin system.
 *
 * Defines PluginManifest, Plugin interface, PluginContext, and related types.
 */

import type { Container } from "@wf-agent/common-utils";
import type { ContributionRegistrar } from "./contributions/registrar.js";
import type { ContributionType } from "./contributions/types.js";

export type { ContributionType }; // re-export for backward compatibility
import type { ContributionManager } from "./contributions/manager.js";
import type { OverridePolicy } from "./config.js";

// ============================================================
// Plugin Manifest
// ============================================================

/**
 * Plugin manifest describing metadata and capabilities.
 */
export interface PluginManifest {
  /** Unique plugin identifier (e.g. "@scope/my-plugin") */
  id: string;
  /** Semver version string */
  version: string;
  /** Human-readable name */
  name?: string;
  /** Short description */
  description?: string;
  /** Semver range constraint for SDK compatibility (e.g. ">=1.0.0 <2.0.0") */
  sdkVersion: string;
  /** Path to the plugin entry point module */
  entryPoint: string;
  /** Declared plugin dependencies (plugin-id -> semver range) */
  dependencies?: Record<string, string>;
  /** Optional dependencies */
  optionalDependencies?: Record<string, string>;
  /** JSON Schema for plugin configuration */
  config?: Record<string, unknown>;
  /** List of contribution types this plugin provides */
  contributions?: ContributionType[];
  /** Lifecycle hook method names */
  hooks?: Partial<Record<PluginLifecycleHook, string>>;
  /** Path to the plugin directory (set by loader) */
  _basePath?: string;
}

// ============================================================
// Lifecycle Hooks
// ============================================================

export type PluginLifecycleHook =
  | 'onLoad'
  | 'onUnload'
  | 'onActivate'
  | 'onDeactivate'
  | 'onConfigChange';

// ============================================================
// Plugin Context
// ============================================================

export interface PluginLogger {
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
  debug(message: string, ...args: unknown[]): void;
}

/**
 * Context provided to plugin lifecycle hooks.
 */
export interface PluginContext {
  /** SDK version string */
  sdkVersion: string;
  /** DI container for service access */
  container: Container;
  /** Logger scoped to this plugin */
  logger: PluginLogger;
  /** Plugin-specific configuration */
  config: Record<string, unknown>;
  /** Contribution manager for querying other plugin contributions */
  contributionManager: ContributionManager;
  /** Current plugin ID */
  pluginId: string;
}

// ============================================================
// Plugin Interface
// ============================================================

/**
 * Plugin interface that all plugins must implement (default export).
 */
export interface Plugin {
  /** Plugin manifest metadata */
  manifest: PluginManifest;

  // Lifecycle hooks
  onLoad?(context: PluginContext): Promise<void>;
  onUnload?(context: PluginContext): Promise<void>;
  onActivate?(context: PluginContext): Promise<void>;
  onDeactivate?(context: PluginContext): Promise<void>;
  onConfigChange?(config: Record<string, unknown>): Promise<void>;

  /**
   * Contribution registration (called during onLoad).
   *
   * @deprecated Use the new sub-interfaces via `registrar.nodeTypes` etc.
   *             Instead of `registrar.registerNodeType(...)`, use
   *             `registrar.nodeTypes?.registerNodeType(...)`.
   */
  registerContributions?(registrar: ContributionRegistrar): void;
}

// ============================================================
// Plugin Status
// ============================================================

export enum PluginStatus {
  DISCOVERED = 'discovered',
  LOADING = 'loading',
  LOADED = 'loaded',
  ACTIVATING = 'activating',
  ACTIVE = 'active',
  DEACTIVATING = 'deactivating',
  DEACTIVATED = 'deactivated',
  ERROR = 'error',
}

// ============================================================
// Plugin Record (Registry Entry)
// ============================================================

export interface ContributionRecord {
  type: ContributionType;
  key: string;
  pluginId: string;
}

export interface PluginRecord {
  manifest: PluginManifest;
  instance: Plugin;
  status: PluginStatus;
  contributions: ContributionRecord[];
  dependencies: Map<string, PluginRecord>;
  dependents: Set<PluginRecord>;
  activatedAt?: number;
  error?: Error;
}

// ============================================================
// Discovery
// ============================================================

export interface DiscoveredPlugin {
  manifest: PluginManifest;
  sourcePath: string;
}

// ============================================================
// Validation
// ============================================================

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

// ============================================================
// Plugin Engine Options
// ============================================================

export interface PluginSystemOptions {
  /** Enable the plugin system */
  enabled: boolean;
  /** Directories to scan for plugins */
  paths?: string[];
  /** Auto-load discovered plugins (default: true) */
  autoLoad?: boolean;
  /** Auto-activate after loading (default: true) */
  autoActivate?: boolean;
  /** Max execution time per lifecycle hook (ms, 0 = no limit, default 10000) */
  guardTimeout?: number;
  /** How to handle contribution conflicts */
  overridePolicy?: OverridePolicy;
  /** Explicit plugin allowlist (only these plugins are loaded) */
  allowList?: string[];
  /** Plugin blocklist (these plugins are skipped) */
  blockList?: string[];
  /** Plugin-specific configurations */
  config?: Record<string, Record<string, unknown>>;
}

export interface PluginEngineOptions {
  /** Plugin system configuration */
  plugins: PluginSystemOptions;
  /** SDK version string for compatibility checking */
  sdkVersion: string;
}

// ============================================================
// Resolved Dependency Graph
// ============================================================

export interface ResolvedDependencyGraph {
  /** Topological load order (plugin IDs) */
  loadOrder: string[];
  /** Dependency adjacency: dependency -> dependents */
  adjacency: Map<string, string[]>;
  /** Detected cycles (if any) */
  cycles: string[][];
  /** Unresolved dependency plugin IDs */
  missing: string[];
}