/**
 * SQLite Connection Pool Manager
 * Manages shared database connections to reduce resource usage
 */

import Database from "better-sqlite3";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-pool");

/**
 * Connection pool configuration
 */
export interface ConnectionPoolConfig {
  /** Maximum number of connections per database (default: 1 for SQLite) */
  maxConnections?: number;
  /** Enable WAL mode for better concurrent read performance */
  enableWAL?: boolean;
  /** Timeout for acquiring connection in milliseconds (default: 5000) */
  acquireTimeoutMs?: number;
  /** Enable connection health checks */
  enableHealthCheck?: boolean;
  /** Health check interval in milliseconds (default: 60000) */
  healthCheckIntervalMs?: number;
}

/**
 * Managed database connection with reference counting
 */
interface ManagedConnection {
  db: Database.Database;
  path: string;
  refCount: number;
  createdAt: number;
  lastHealthCheck?: number;
  isHealthy: boolean;
}

/**
 * SQLite Connection Pool
 * Provides shared connections across multiple storage adapters
 * Note: SQLite is single-writer, so we typically use 1 connection per database file
 */
export class SqliteConnectionPool {
  private connections: Map<string, ManagedConnection> = new Map();
  private readonly config: Required<ConnectionPoolConfig>;

  constructor(config: ConnectionPoolConfig = {}) {
    this.config = {
      maxConnections: config.maxConnections ?? 1, // SQLite typically uses 1 connection
      enableWAL: config.enableWAL ?? true,
      acquireTimeoutMs: config.acquireTimeoutMs ?? 5000,
      enableHealthCheck: config.enableHealthCheck ?? false,
      healthCheckIntervalMs: config.healthCheckIntervalMs ?? 60000,
    };
  }

  /**
   * Get or create a connection for the given database path
   * Returns a shared connection if one already exists
   */
  getConnection(dbPath: string): Database.Database {
    const existing = this.connections.get(dbPath);
    
    if (existing) {
      // Perform health check if enabled
      if (this.config.enableHealthCheck && !this.isConnectionHealthy(existing)) {
        logger.warn("Unhealthy connection detected, recreating", { dbPath });
        try {
          existing.db.close();
        } catch (error) {
          logger.error("Error closing unhealthy connection", { dbPath, error: (error as Error).message });
        }
        this.connections.delete(dbPath);
        return this.createConnection(dbPath);
      }
      
      existing.refCount++;
      logger.debug("Reusing existing SQLite connection", {
        dbPath,
        refCount: existing.refCount,
      });
      return existing.db;
    }

    return this.createConnection(dbPath);
  }

  /**
   * Create a new database connection
   */
  private createConnection(dbPath: string): Database.Database {
    logger.debug("Creating new SQLite connection", { dbPath });
    const db = new Database(dbPath);

    // Configure connection
    if (this.config.enableWAL) {
      db.pragma("journal_mode = WAL");
      logger.debug("Enabled WAL mode", { dbPath });
    } else {
      db.pragma("journal_mode = DELETE");
      logger.debug("Disabled WAL mode (using DELETE)", { dbPath });
    }

    // Common optimizations
    db.pragma("foreign_keys = ON");
    db.pragma("cache_size = -64000"); // 64MB cache
    db.pragma("temp_store = MEMORY");

    const managedConn: ManagedConnection = {
      db,
      path: dbPath,
      refCount: 1,
      createdAt: Date.now(),
      isHealthy: true,
    };

    this.connections.set(dbPath, managedConn);

    logger.info("SQLite connection created and pooled", {
      dbPath,
      walMode: this.config.enableWAL,
    });

    return db;
  }

  /**
   * Check if a connection is healthy
   */
  private isConnectionHealthy(conn: ManagedConnection): boolean {
    if (!conn.isHealthy) {
      return false;
    }

    // Skip health check if it was performed recently
    if (conn.lastHealthCheck) {
      const timeSinceLastCheck = Date.now() - conn.lastHealthCheck;
      if (timeSinceLastCheck < this.config.healthCheckIntervalMs) {
        return true;
      }
    }

    // Perform a simple query to verify connection
    try {
      conn.db.pragma('quick_check');
      conn.lastHealthCheck = Date.now();
      conn.isHealthy = true;
      return true;
    } catch (error) {
      logger.error("Connection health check failed", { 
        dbPath: conn.path, 
        error: (error as Error).message 
      });
      conn.isHealthy = false;
      return false;
    }
  }

  /**
   * Release a connection (decrement reference count)
   * Connection is only closed when refCount reaches 0
   */
  releaseConnection(dbPath: string): void {
    const conn = this.connections.get(dbPath);
    
    if (!conn) {
      logger.warn("Attempted to release non-existent connection", { dbPath });
      return;
    }

    conn.refCount--;
    logger.debug("Connection released", {
      dbPath,
      refCount: conn.refCount,
    });

    // Close connection when no longer referenced
    if (conn.refCount <= 0) {
      logger.info("Closing SQLite connection (refCount=0)", { dbPath });
      try {
        conn.db.close();
      } catch (error) {
        logger.error("Error closing SQLite connection", {
          dbPath,
          error: (error as Error).message,
        });
      }
      this.connections.delete(dbPath);
    }
  }

  /**
   * Check if a connection exists for the given path
   */
  hasConnection(dbPath: string): boolean {
    return this.connections.has(dbPath);
  }

  /**
   * Get connection statistics
   */
  getStats(): {
    totalConnections: number;
    activeConnections: number;
    connections: Array<{
      path: string;
      refCount: number;
      age: number;
    }>;
  } {
    const connections = Array.from(this.connections.values()).map(conn => ({
      path: conn.path,
      refCount: conn.refCount,
      age: Date.now() - conn.createdAt,
    }));

    return {
      totalConnections: this.connections.size,
      activeConnections: connections.filter(c => c.refCount > 0).length,
      connections,
    };
  }

  /**
   * Close all connections immediately
   * Use this for cleanup/shutdown
   */
  closeAll(): void {
    logger.info("Closing all SQLite connections", {
      count: this.connections.size,
    });

    for (const [dbPath, conn] of this.connections.entries()) {
      try {
        conn.db.close();
        logger.debug("Connection closed", { dbPath });
      } catch (error) {
        logger.error("Error closing connection during shutdown", {
          dbPath,
          error: (error as Error).message,
        });
      }
    }

    this.connections.clear();
    logger.info("All SQLite connections closed");
  }

  /**
   * Get pool size (number of tracked connections)
   */
  get size(): number {
    return this.connections.size;
  }
}

/**
 * Global connection pool instance
 * Singleton pattern for shared connection management
 */
let globalPool: SqliteConnectionPool | null = null;

/**
 * Get or create the global connection pool
 */
export function getGlobalConnectionPool(): SqliteConnectionPool {
  if (!globalPool) {
    globalPool = new SqliteConnectionPool();
    logger.info("Global SQLite connection pool created");
  }
  return globalPool;
}

/**
 * Reset the global connection pool (for testing)
 */
export function resetGlobalConnectionPool(): void {
  if (globalPool) {
    globalPool.closeAll();
    globalPool = null;
    logger.info("Global SQLite connection pool reset");
  }
}
