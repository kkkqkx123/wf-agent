/**
 * PostgreSQL Metrics Storage Implementation
 * Stores metrics data in a PostgreSQL database table
 *
 * Database Schema:
 * - metrics table: Stores all metric data points with JSON labels
 * - Indexes on (metric_name, timestamp) for efficient time-range queries
 */

import { Pool } from "pg";
import { createModuleLogger } from "../logger.js";
import {
  PostgresConnectionPool,
  getGlobalConnectionPool,
  type PostgresPoolConfig,
} from "./connection-pool.js";
import type { MetricsStorageAdapter, MetricDataPoint, MetricsQuery } from "../types/adapter/metrics-storage-adapter.js";

const logger = createModuleLogger("postgres-metrics-storage");

export interface PostgresMetricsStorageConfig {
  /** PostgreSQL connection string */
  connectionString: string;
  /** Connection pool configuration */
  poolConfig?: PostgresPoolConfig;
  /** Use shared connection pool (default: true) */
  useConnectionPool?: boolean;
  /** Custom connection pool instance (optional) */
  connectionPool?: PostgresConnectionPool;
}

export class PostgresMetricsStorage implements MetricsStorageAdapter {
  private pool: Pool;
  private poolManager?: PostgresConnectionPool;
  private initialized = false;
  private config: PostgresMetricsStorageConfig;

  constructor(config: PostgresMetricsStorageConfig) {
    this.config = config;

    if (config.connectionPool) {
      // Use provided pool instance
      this.poolManager = config.connectionPool;
      this.pool = this.poolManager.getPool(config.connectionString, config.poolConfig);
    } else if (config.useConnectionPool !== false) {
      // Use global pool
      this.poolManager = getGlobalConnectionPool();
      this.pool = this.poolManager.getPool(config.connectionString, config.poolConfig);
    } else {
      // Create standalone pool
      this.pool = new Pool({ connectionString: config.connectionString, ...config.poolConfig });
    }
  }

  private async getClient() {
    return this.pool.connect();
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      const client = await this.getClient();
      try {
        // Create metrics table
        await client.query(`
          CREATE TABLE IF NOT EXISTS metrics (
            id SERIAL PRIMARY KEY,
            metric_name TEXT NOT NULL,
            metric_type TEXT NOT NULL CHECK(metric_type IN ('counter', 'gauge', 'histogram')),
            value DOUBLE PRECISION NOT NULL,
            timestamp BIGINT NOT NULL,
            labels JSONB DEFAULT NULL,
            collector_name TEXT NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
          )
        `);

        // Create indexes
        await client.query(`
          CREATE INDEX IF NOT EXISTS idx_metrics_name_timestamp
          ON metrics(metric_name, timestamp)
        `);
        await client.query(`
          CREATE INDEX IF NOT EXISTS idx_metrics_timestamp
          ON metrics(timestamp)
        `);
        await client.query(`
          CREATE INDEX IF NOT EXISTS idx_metrics_collector
          ON metrics(collector_name)
        `);

        this.initialized = true;
        logger.info("PostgresMetricsStorage initialized");
      } finally {
        client.release();
      }
    } catch (error) {
      logger.error("Failed to initialize PostgresMetricsStorage", { error });
      throw error;
    }
  }

  async saveBatch(metrics: MetricDataPoint[]): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    if (metrics.length === 0) return;

    const client = await this.getClient();
    try {
      await client.query("BEGIN");

      const insertQuery = `
        INSERT INTO metrics (metric_name, metric_type, value, timestamp, labels, collector_name)
        VALUES ($1, $2, $3, $4, $5, $6)
      `;

      for (const metric of metrics) {
        await client.query(insertQuery, [
          metric.metricName,
          metric.metricType,
          metric.value,
          metric.timestamp,
          metric.labels ? JSON.stringify(metric.labels) : null,
          metric.collectorName,
        ]);
      }

      await client.query("COMMIT");
      logger.debug("Saved metrics batch", { count: metrics.length });
    } catch (error) {
      await client.query("ROLLBACK");
      logger.error("Failed to save metrics batch", {
        count: metrics.length,
        error: (error as Error).message,
      });
      throw error;
    } finally {
      client.release();
    }
  }

  async query(query: MetricsQuery): Promise<MetricDataPoint[]> {
    if (!this.initialized) {
      await this.initialize();
    }

    const client = await this.getClient();
    try {
      let sql = "SELECT * FROM metrics WHERE 1=1";
      const params: Array<string | number> = [];
      let paramIndex = 1;

      if (query.metricName) {
        sql += ` AND metric_name = $${paramIndex++}`;
        params.push(query.metricName);
      }

      if (query.metricType) {
        sql += ` AND metric_type = $${paramIndex++}`;
        params.push(query.metricType);
      }

      if (query.startTime !== undefined) {
        sql += ` AND timestamp >= $${paramIndex++}`;
        params.push(query.startTime);
      }

      if (query.endTime !== undefined) {
        sql += ` AND timestamp <= $${paramIndex++}`;
        params.push(query.endTime);
      }

      if (query.collectorName) {
        sql += ` AND collector_name = $${paramIndex++}`;
        params.push(query.collectorName);
      }

      // Label filtering using JSONB containment
      if (query.labels && Object.keys(query.labels).length > 0) {
        for (const [key, value] of Object.entries(query.labels)) {
          sql += ` AND labels @> $${paramIndex++}`;
          params.push(JSON.stringify({ [key]: value }));
        }
      }

      // Sort order
      const sortOrder = query.sortOrder === "asc" ? "ASC" : "DESC";
      sql += ` ORDER BY timestamp ${sortOrder}`;

      // Limit
      if (query.limit && query.limit > 0) {
        sql += ` LIMIT $${paramIndex++}`;
        params.push(query.limit);
      }

      const result = await client.query(sql, params);

      return result.rows.map(row => ({
        metricName: row.metric_name,
        metricType: row.metric_type,
        value: row.value,
        timestamp: Number(row.timestamp),
        labels: row.labels || undefined,
        collectorName: row.collector_name,
      }));
    } catch (error) {
      logger.error("Failed to query metrics", { error });
      throw error;
    } finally {
      client.release();
    }
  }

  async deleteOldMetrics(beforeTimestamp: number): Promise<number> {
    if (!this.initialized) {
      await this.initialize();
    }

    const client = await this.getClient();
    try {
      const result = await client.query(
        "DELETE FROM metrics WHERE timestamp < $1",
        [beforeTimestamp],
      );

      logger.info("Deleted old metrics", {
        count: result.rowCount,
        beforeTimestamp,
      });

      return result.rowCount ?? 0;
    } catch (error) {
      logger.error("Failed to delete old metrics", { error });
      throw error;
    } finally {
      client.release();
    }
  }

  async close(): Promise<void> {
    if (!this.config.connectionPool && this.config.useConnectionPool === false) {
      await this.pool.end();
    }
    this.initialized = false;
    logger.info("PostgresMetricsStorage closed");
  }
}
