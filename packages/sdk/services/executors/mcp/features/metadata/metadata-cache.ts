/**
 * MCP Tool Metadata Cache
 *
 * Caches MCP server tool and resource metadata to avoid repeated listTools/listResources calls.
 * Follows the pattern of ephemeral dynamic context from packages/types/src/dynamic-context.ts
 * but with persistence across a bounded time window.
 */

import type { McpServerState } from "../../types.js";
import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpToolMetadataCache" });

/**
 * Cached metadata entry with timestamp
 */
interface CacheEntry {
  /** Server state (containing tools, resources, etc.) */
  state: McpServerState;
  /** Cache timestamp (milliseconds since epoch) */
  timestamp: number;
  /** Time-to-live in milliseconds */
  ttl: number;
}

/**
 * MCP Tool Metadata Cache Configuration
 */
export interface McpToolMetadataCacheConfig {
  /** Default TTL for cache entries in milliseconds (default: 5 minutes = 300000) */
  defaultTtl?: number;
  /** Maximum cache size (entries per server, default: 1) */
  maxSize?: number;
  /** Whether to enable automatic cleanup of expired entries (default: true) */
  enableAutoCleanup?: boolean;
  /** Cleanup interval in milliseconds (default: 1 minute = 60000) */
  cleanupInterval?: number;
  /** Callback when a cache entry expires */
  onExpire?: (serverName: string) => void;
}

/**
 * MCP Tool Metadata Cache
 *
 * Manages caching of MCP server metadata (tools, resources, instructions).
 * Useful for reducing repeated listTools/listResources calls.
 *
 * Thread-safe via simple Map without external locking (single-threaded JS).
 */
export class McpToolMetadataCache {
  private cache = new Map<string, CacheEntry>();
  private config: {
    defaultTtl: number;
    maxSize: number;
    enableAutoCleanup: boolean;
    cleanupInterval: number;
    onExpire?: (serverName: string) => void;
  };
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;

  constructor(config?: McpToolMetadataCacheConfig) {
    this.config = {
      defaultTtl: config?.defaultTtl ?? 5 * 60 * 1000, // 5 minutes
      maxSize: config?.maxSize ?? 1,
      enableAutoCleanup: config?.enableAutoCleanup ?? true,
      cleanupInterval: config?.cleanupInterval ?? 60 * 1000, // 1 minute
      onExpire: config?.onExpire,
    };

    if (this.config.enableAutoCleanup) {
      this.startAutoCleanup();
    }

    logger.debug("MetadataCache initialized", { config: this.config });
  }

  /**
   * Start automatic cleanup timer
   */
  private startAutoCleanup(): void {
    if (this.cleanupTimer) return;

    this.cleanupTimer = setInterval(() => {
      const removed = this.cleanup();
      if (removed > 0) {
        logger.debug("Auto-cleanup removed expired entries", { count: removed });
      }
    }, this.config.cleanupInterval);

    logger.debug("Auto-cleanup timer started", { interval: this.config.cleanupInterval });
  }

  /**
   * Check if cache entry is expired
   */
  private isExpired(entry: CacheEntry): boolean {
    const now = Date.now();
    return now - entry.timestamp > entry.ttl;
  }

  /**
   * Set cached metadata for a server
   *
   * @param serverName - Server name
   * @param state - Server state containing tools/resources
   * @param ttl - Time-to-live in milliseconds (optional, uses default if not provided)
   */
  set(serverName: string, state: McpServerState, ttl?: number): void {
    const entry: CacheEntry = {
      state,
      timestamp: Date.now(),
      ttl: ttl ?? this.config.defaultTtl,
    };

    this.cache.set(serverName, entry);
    logger.debug("Metadata cached", {
      server: serverName,
      tools: state.tools?.length ?? 0,
      resources: state.resources?.length ?? 0,
      ttl: entry.ttl,
    });
  }

  /**
   * Get cached metadata for a server
   *
   * @param serverName - Server name
   * @returns Cached server state, or undefined if not found or expired
   */
  get(serverName: string): McpServerState | undefined {
    const entry = this.cache.get(serverName);
    if (!entry) return undefined;

    if (this.isExpired(entry)) {
      this.cache.delete(serverName);
      logger.debug("Cache expired for server", { server: serverName });
      return undefined;
    }

    return entry.state;
  }

  /**
   * Check if metadata is cached and valid
   *
   * @param serverName - Server name
   * @returns true if valid cache entry exists
   */
  has(serverName: string): boolean {
    const entry = this.cache.get(serverName);
    if (!entry) return false;

    if (this.isExpired(entry)) {
      this.cache.delete(serverName);
      return false;
    }

    return true;
  }

  /**
   * Get all cached servers
   *
   * @returns Array of server names with valid cache entries
   */
  keys(): string[] {
    const validKeys: string[] = [];

    for (const [key, entry] of this.cache) {
      if (!this.isExpired(entry)) {
        validKeys.push(key);
      } else {
        this.cache.delete(key);
      }
    }

    return validKeys;
  }

  /**
   * Get all cached entries
   *
   * @returns Map of server name to cached state
   */
  entries(): Map<string, McpServerState> {
    const result = new Map<string, McpServerState>();

    for (const [name, entry] of this.cache) {
      if (!this.isExpired(entry)) {
        result.set(name, entry.state);
      } else {
        this.cache.delete(name);
      }
    }

    return result;
  }

  /**
   * Invalidate cache for a specific server
   *
   * @param serverName - Server name
   */
  invalidate(serverName: string): void {
    this.cache.delete(serverName);
    logger.debug("Cache invalidated for server", { server: serverName });
  }

  /**
   * Invalidate cache for multiple servers
   *
   * @param serverNames - Array of server names
   */
  invalidateMultiple(serverNames: string[]): void {
    for (const name of serverNames) {
      this.cache.delete(name);
    }
    logger.debug("Cache invalidated for servers", { count: serverNames.length });
  }

  /**
   * Clear all cached entries
   */
  clear(): void {
    const count = this.cache.size;
    this.cache.clear();
    logger.debug("Cache cleared", { count });
  }

  /**
   * Remove all expired entries
   *
   * @returns Number of entries removed
   */
  cleanup(): number {
    let removed = 0;

    for (const [key, entry] of this.cache) {
      if (this.isExpired(entry)) {
        this.cache.delete(key);
        removed++;

        // Trigger expiration callback for this server
        if (this.config.onExpire) {
          try {
            this.config.onExpire(key);
          } catch (error) {
            logger.error(`Error in onExpire callback for server "${key}":`, { error });
          }
        }
      }
    }

    return removed;
  }

  /**
   * Get cache statistics
   *
   * @returns Cache statistics
   */
  getStats(): {
    size: number;
    servers: Array<{
      name: string;
      tools: number;
      resources: number;
      ttl: number;
      expiresIn: number;
    }>;
  } {
    const now = Date.now();
    const servers: Array<{
      name: string;
      tools: number;
      resources: number;
      ttl: number;
      expiresIn: number;
    }> = [];

    for (const [name, entry] of this.cache) {
      if (!this.isExpired(entry)) {
        servers.push({
          name,
          tools: entry.state.tools?.length ?? 0,
          resources: entry.state.resources?.length ?? 0,
          ttl: entry.ttl,
          expiresIn: entry.ttl - (now - entry.timestamp),
        });
      }
    }

    return {
      size: servers.length,
      servers,
    };
  }

  /**
   * Shutdown cache and cleanup resources
   */
  shutdown(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    this.cache.clear();
    logger.debug("MetadataCache shutdown");
  }
}
