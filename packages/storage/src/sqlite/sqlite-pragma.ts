/**
 * Shared SQLite PRAGMA configuration utilities
 *
 * Centralizes common PRAGMA settings to ensure consistency across all
 * SQLite storage implementations (BaseSqliteStorage subclasses, standalone
 * stores, and connection pool).
 */

import type Database from "better-sqlite3";

/**
 * Standard PRAGMA configuration options
 */
export interface PragmaConfig {
  /** Enable WAL mode (default: true) */
  enableWAL?: boolean;
  /** Auto-vacuum mode (default: 'INCREMENTAL') */
  autoVacuum?: 'NONE' | 'FULL' | 'INCREMENTAL';
  /** Journal size limit in bytes (default: 64MB) */
  journalSizeLimit?: number;
  /** Busy timeout in ms (default: 5000) */
  busyTimeout?: number;
  /** Cache size in pages (negative = KB, default: -64000 = 64MB) */
  cacheSize?: number;
  /** Temp store location (default: 'MEMORY') */
  tempStore?: 'FILE' | 'MEMORY';
  /** Synchronous mode (default: 'NORMAL') */
  synchronous?: 'OFF' | 'NORMAL' | 'FULL' | 'EXTRA';
  /** Enable foreign keys (default: true) */
  foreignKeys?: boolean;
  /** WAL auto-checkpoint interval in pages (default: 1000) */
  walAutocheckpoint?: number;
}

const DEFAULTS: Required<PragmaConfig> = {
  enableWAL: true,
  autoVacuum: 'INCREMENTAL',
  journalSizeLimit: 64 * 1024 * 1024, // 64MB
  busyTimeout: 5000,
  cacheSize: -64000, // 64MB
  tempStore: 'MEMORY',
  synchronous: 'NORMAL',
  foreignKeys: true,
  walAutocheckpoint: 1000,
};

/**
 * Apply standard PRAGMA configuration to a database connection
 * Call this right after new Database() and before any schema operations
 */
export function configurePragmas(db: Database.Database, config?: PragmaConfig): void {
  const cfg = { ...DEFAULTS, ...config };

  if (cfg.enableWAL) {
    db.pragma('journal_mode = WAL');
  }

  if (cfg.autoVacuum !== 'NONE') {
    db.pragma(`auto_vacuum = ${cfg.autoVacuum}`);
  }

  db.pragma(`journal_size_limit = ${cfg.journalSizeLimit}`);
  db.pragma(`busy_timeout = ${cfg.busyTimeout}`);
  db.pragma(`cache_size = ${cfg.cacheSize}`);
  db.pragma(`temp_store = ${cfg.tempStore}`);
  db.pragma(`synchronous = ${cfg.synchronous}`);
  db.pragma(`wal_autocheckpoint = ${cfg.walAutocheckpoint}`);

  if (cfg.foreignKeys) {
    db.pragma('foreign_keys = ON');
  }
}