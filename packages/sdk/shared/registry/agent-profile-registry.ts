/**
 * Agent Profile Registry
 *
 * Manages the registration and discovery of agent profile metadata.
 * Provides a single source of truth for available agent profiles,
 * used by the call_agent tool for validation and description injection.
 *
 * Design follows ProfileManager (LLM Profile) pattern:
 * - Simple in-memory Map storage (agent profiles don't need graph preprocessing)
 * - Supports register, get, list, has, remove operations
 * - Stores only metadata (id, name, description); full config loading is
 *   delegated to the config loader (file path or inline definition)
 *
 * @see ProfileManager - Similar pattern for LLM Profile management
 */

import type { AgentProfileStorageAdapter } from "@wf-agent/storage";
import {
  persistAgentProfile,
  removeAgentProfile,
  initializeAgentProfilesFromStorage,
} from "./utils/storage/index.js";
import { createRegistry } from "./utils/index.js";
import { validateRequiredString } from "./utils/index.js";

/**
 * Agent profile metadata stored in the registry
 */
export interface AgentProfileMeta {
  /** Unique profile identifier */
  id: string;
  /** Human-readable name */
  name: string;
  /** Optional description of the agent's purpose and capabilities */
  description?: string;
}

/**
 * Agent Profile Registry Class
 */
export class AgentProfileRegistry {
  private items = createRegistry<AgentProfileMeta>();

  constructor(private readonly storageAdapter: AgentProfileStorageAdapter | null = null) {}

  /**
   * Register an agent profile (memory-only, no persistence).
   *
   * @param profile Agent profile metadata to register
   */
  register(profile: AgentProfileMeta): void {
    this.validateProfile(profile);
    this.items.set(profile.id, { ...profile });
  }

  /**
   * Register an agent profile with storage persistence (write-through).
   *
   * @param profile Agent profile metadata to register
   */
  async registerProfile(profile: AgentProfileMeta): Promise<void> {
    this.validateProfile(profile);

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistAgentProfile(profile, this.storageAdapter);
    }

    this.items.set(profile.id, { ...profile });
  }

  /**
   * Get an agent profile by ID
   *
   * @param id Profile ID
   * @returns Agent profile metadata or undefined if not found
   */
  get(id: string): AgentProfileMeta | undefined {
    return this.items.get(id);
  }

  /**
   * List all registered agent profiles
   *
   * @returns Array of agent profile metadata
   */
  list(): AgentProfileMeta[] {
    return this.items.list();
  }

  /**
   * Remove an agent profile from the registry (memory only).
   *
   * @param id Profile ID to remove
   */
  remove(id: string): void {
    this.items.delete(id);
  }

  /**
   * Remove an agent profile with storage persistence (write-through).
   *
   * @param id Profile ID to remove
   */
  async removeProfile(id: string): Promise<void> {
    // Remove from storage first (write-through)
    if (this.storageAdapter) {
      await removeAgentProfile(id, this.storageAdapter);
    }

    this.items.delete(id);
  }

  /**
   * Check if an agent profile exists
   *
   * @param id Profile ID
   * @returns Whether the profile exists
   */
  has(id: string): boolean {
    return this.items.has(id);
  }

  /**
   * Get the number of registered agent profiles
   *
   * @returns Number of profiles
   */
  size(): number {
    return this.items.size;
  }

  /**
   * Clear all registered agent profiles
   */
  async clear(): Promise<void> {
    this.items.clear();
    if (this.storageAdapter) {
      await this.storageAdapter.clear();
    }
  }

  // ============================================================
  // Storage Operations
  // ============================================================

  /**
   * Initialize agent profiles from storage.
   * Loads all persisted profiles into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeAgentProfilesFromStorage(this.storageAdapter, this.items);
  }

  /**
   * Validate profile metadata before registration
   *
   * @param profile Profile metadata to validate
   */
  private validateProfile(profile: AgentProfileMeta): void {
    validateRequiredString(profile as unknown as Record<string, unknown>, "id", "Agent profile ID is required and must be a string");
    validateRequiredString(profile as unknown as Record<string, unknown>, "name", "Agent profile name is required and must be a string");
  }
}
