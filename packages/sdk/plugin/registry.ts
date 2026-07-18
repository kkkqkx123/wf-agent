/**
 * Plugin Registry - Manages active plugin records with status tracking.
 *
 * Responsibilities:
 * - Register, query, and remove plugin records
 * - Status-based querying
 * - Contribution-based querying
 * - Dependency tracking between plugins
 */

import type { Plugin, PluginManifest, PluginRecord, ContributionRecord, ContributionType, PluginStatus } from "./types.js";
import { createRegistry } from "../shared/registry/utils/registry-internals.js";
import type { MutableRegistry } from "../shared/registry/types.js";

/**
 * Plugin Registry - Central registry for all plugin records.
 */
export class PluginRegistry {
  private plugins: MutableRegistry<PluginRecord>;

  constructor() {
    this.plugins = createRegistry<PluginRecord>();
  }

  /**
   * Register a plugin with its manifest and instance.
   */
  register(manifest: PluginManifest, instance: Plugin): PluginRecord {
    const record: PluginRecord = {
      manifest,
      instance,
      status: 'discovered' as PluginStatus,
      contributions: [],
      dependencies: new Map(),
      dependents: new Set(),
    };
    this.plugins.set(manifest.id, record);
    return record;
  }

  /**
   * Get a plugin record by ID.
   */
  get(pluginId: string): PluginRecord | undefined {
    return this.plugins.get(pluginId);
  }

  /**
   * Check if a plugin is registered.
   */
  has(pluginId: string): boolean {
    return this.plugins.has(pluginId);
  }

  /**
   * List all registered plugin records.
   */
  list(): PluginRecord[] {
    return this.plugins.list();
  }

  /**
   * List all plugin IDs.
   */
  listIds(): string[] {
    return this.plugins.keys();
  }

  /**
   * List plugins by status.
   */
  listByStatus(status: PluginStatus): PluginRecord[] {
    return this.list().filter(p => p.status === status);
  }

  /**
   * List plugins by contribution type.
   */
  listByContribution(type: ContributionType): PluginRecord[] {
    return this.list().filter(p => p.contributions.some(c => c.type === type));
  }

  /**
   * Update the status of a plugin.
   */
  updateStatus(pluginId: string, status: PluginStatus): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      record.status = status;
    }
  }

  /**
   * Set the error on a plugin record.
   */
  setError(pluginId: string, error: Error): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      record.error = error;
      record.status = 'error' as PluginStatus;
    }
  }

  /**
   * Add contributions to a plugin record.
   */
  addContributions(pluginId: string, contributions: ContributionRecord[]): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      record.contributions.push(...contributions);
    }
  }

  /**
   * Set dependency relationships for a plugin.
   */
  setDependencies(pluginId: string, deps: Map<string, PluginRecord>): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      record.dependencies = deps;
      // Register this plugin as dependent on each dependency
      for (const [, depRecord] of deps) {
        depRecord.dependents.add(record);
      }
    }
  }

  /**
   * Set activated timestamp.
   */
  setActivatedAt(pluginId: string, timestamp: number): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      record.activatedAt = timestamp;
    }
  }

  /**
   * Remove a plugin record and clean up dependency references.
   */
  remove(pluginId: string): void {
    const record = this.plugins.get(pluginId);
    if (record) {
      // Remove this plugin from its dependents' dependency maps
      for (const [, depRecord] of record.dependencies) {
        depRecord.dependents.delete(record);
      }
      this.plugins.delete(pluginId);
    }
  }

  /**
   * Clear all plugin records.
   */
  clear(): void {
    this.plugins.clear();
  }
}