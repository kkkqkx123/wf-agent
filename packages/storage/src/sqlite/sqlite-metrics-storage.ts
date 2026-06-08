/**
 * SQLite Metrics Storage Implementation
 * 
 * Implements MetricsStorageAdapter using SQLite database.
 * Uses a simple single-table design for efficient storage and querying.
 * 
 * Database Schema:
 * - metrics table: Stores all metric data points with JSON labels
 * - Indexes on (metric_name, timestamp) for efficient time-range queries
 */

import Database from 'better-sqlite3';
import { join } from 'path';
import { createModuleLogger } from '../logger.js';
import { configurePragmas } from './sqlite-pragma.js';
import type { MetricsStorageAdapter, MetricDataPoint, MetricsQuery } from '../types/adapter/metrics-storage-adapter.js';

const logger = createModuleLogger('sqlite-metrics-storage');

export interface SqliteMetricsStorageConfig {
  /** Path to SQLite database file */
  dbPath?: string;
  /** Enable WAL mode for better concurrent performance */
  enableWAL?: boolean;
  /** Auto-vacuum interval in milliseconds (0 to disable, default: 1 hour) */
  vacuumInterval?: number;
}

export class SqliteMetricsStorage implements MetricsStorageAdapter {
  private db!: Database.Database;
  private dbPath: string;
  private enableWAL: boolean;
  private vacuumInterval: number;
  private vacuumTimer?: NodeJS.Timeout;
  private initialized = false;

  constructor(config: SqliteMetricsStorageConfig = {}) {
    this.dbPath = config.dbPath || join(process.cwd(), 'data', 'metrics.db');
    this.enableWAL = config.enableWAL ?? true;
    this.vacuumInterval = config.vacuumInterval ?? 60 * 60 * 1000; // 1 hour
  }

  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      // Ensure directory exists
      const { mkdirSync } = await import('fs');
      const { dirname } = await import('path');
      mkdirSync(dirname(this.dbPath), { recursive: true });

      // Initialize SQLite database
      this.db = new Database(this.dbPath);
      
      // Configure database using shared PRAGMA utility
      configurePragmas(this.db, {
        enableWAL: this.enableWAL,
        autoVacuum: 'INCREMENTAL',
        journalSizeLimit: 64 * 1024 * 1024,
        busyTimeout: 5000,
      });

      // Create tables and indexes
      this.createSchema();

      // Start auto-vacuum timer
      if (this.vacuumInterval > 0) {
        this.vacuumTimer = setInterval(() => {
          this.vacuum().catch(err => {
            logger.warn('Auto-vacuum failed', { error: err });
          });
        }, this.vacuumInterval);
      }

      this.initialized = true;
      logger.info('SQLite metrics storage initialized', { dbPath: this.dbPath });
    } catch (error) {
      logger.error('Failed to initialize SQLite metrics storage', { error });
      throw error;
    }
  }

  async saveBatch(metrics: MetricDataPoint[]): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    if (metrics.length === 0) {
      return;
    }

    try {
      const insertStmt = this.db.prepare(`
        INSERT INTO metrics (
          metric_name, metric_type, value, timestamp, labels, collector_name
        ) VALUES (?, ?, ?, ?, ?, ?)
      `);

      const transaction = this.db.transaction((batch: MetricDataPoint[]) => {
        for (const metric of batch) {
          insertStmt.run(
            metric.metricName,
            metric.metricType,
            metric.value,
            metric.timestamp,
            metric.labels ? JSON.stringify(metric.labels) : null,
            metric.collectorName
          );
        }
      });

      transaction(metrics);

      logger.debug('Saved metrics batch', { count: metrics.length });
    } catch (error) {
      logger.error('Failed to save metrics batch', { 
        count: metrics.length,
        error: (error as Error).message 
      });
      throw error;
    }
  }

  async query(query: MetricsQuery): Promise<MetricDataPoint[]> {
    if (!this.initialized) {
      await this.initialize();
    }

    try {
      // Build dynamic query
      let sql = 'SELECT * FROM metrics WHERE 1=1';
      const params: Array<string | number> = [];

      if (query.metricName) {
        sql += ' AND metric_name = ?';
        params.push(query.metricName);
      }

      if (query.metricType) {
        sql += ' AND metric_type = ?';
        params.push(query.metricType);
      }

      if (query.startTime) {
        sql += ' AND timestamp >= ?';
        params.push(query.startTime);
      }

      if (query.endTime) {
        sql += ' AND timestamp <= ?';
        params.push(query.endTime);
      }

      if (query.collectorName) {
        sql += ' AND collector_name = ?';
        params.push(query.collectorName);
      }

      // Label filtering (requires JSON extraction)
      if (query.labels && Object.keys(query.labels).length > 0) {
        for (const [key, value] of Object.entries(query.labels)) {
          sql += ' AND json_extract(labels, ?) = ?';
          params.push(`$.${key}`);
          params.push(value);
        }
      }

      // Sort order
      const sortOrder = query.sortOrder === 'asc' ? 'ASC' : 'DESC';
      sql += ` ORDER BY timestamp ${sortOrder}`;

      // Limit
      if (query.limit && query.limit > 0) {
        sql += ' LIMIT ?';
        params.push(query.limit);
      }

      const rows = this.db.prepare(sql).all(...params) as Array<{
        id: number;
        metric_name: string;
        metric_type: string;
        value: number;
        timestamp: number;
        labels: string | null;
        collector_name: string;
      }>;

      // Convert to MetricDataPoint format
      return rows.map(row => ({
        metricName: row.metric_name,
        metricType: row.metric_type as 'counter' | 'gauge' | 'histogram',
        value: row.value,
        timestamp: row.timestamp,
        labels: row.labels ? JSON.parse(row.labels) : undefined,
        collectorName: row.collector_name,
      }));
    } catch (error) {
      logger.error('Failed to query metrics', { error });
      throw error;
    }
  }

  async deleteOldMetrics(beforeTimestamp: number): Promise<number> {
    if (!this.initialized) {
      await this.initialize();
    }

    try {
      const result = this.db.prepare(
        'DELETE FROM metrics WHERE timestamp < ?'
      ).run(beforeTimestamp);

      logger.info('Deleted old metrics', { 
        count: result.changes,
        beforeTimestamp 
      });

      return result.changes;
    } catch (error) {
      logger.error('Failed to delete old metrics', { error });
      throw error;
    }
  }

  async close(): Promise<void> {
    if (this.vacuumTimer) {
      clearInterval(this.vacuumTimer);
      this.vacuumTimer = undefined;
    }

    if (this.db) {
      try {
        this.db.close();
        this.initialized = false;
        logger.info('SQLite metrics storage closed');
      } catch (error) {
        logger.error('Failed to close database', { error });
      }
    }
  }

  /**
   * Create database schema (tables and indexes)
   */
  private createSchema(): void {
    // Create metrics table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS metrics (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        metric_name TEXT NOT NULL,
        metric_type TEXT NOT NULL CHECK(metric_type IN ('counter', 'gauge', 'histogram')),
        value REAL NOT NULL,
        timestamp INTEGER NOT NULL,
        labels TEXT,
        collector_name TEXT NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
      )
    `);

    // Create indexes for common query patterns
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_metrics_name_timestamp 
      ON metrics(metric_name, timestamp)
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_metrics_timestamp 
      ON metrics(timestamp)
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_metrics_collector 
      ON metrics(collector_name)
    `);

    logger.debug('Database schema created successfully');
  }

  /**
   * Run VACUUM to reclaim disk space
   */
  private async vacuum(): Promise<void> {
    try {
      this.db.exec('VACUUM');
      logger.debug('Database vacuum completed');
    } catch (error) {
      logger.error('Vacuum failed', { error });
    }
  }
}
