/**
 * Configuration Helpers Tests
 */

import { describe, it, expect } from 'vitest';
import { 
  convertToPostgresBaseConfig, 
  convertToSqliteBaseConfig 
} from '../config-helpers.js';
import type { PostgresStorageConfig, SqliteStorageConfig } from '@wf-agent/types';

describe('Configuration Helpers', () => {
  describe('convertToPostgresBaseConfig', () => {
    it('should convert basic PostgreSQL config', () => {
      const config: PostgresStorageConfig = {
        host: 'localhost',
        username: 'testuser',
        password: 'testpass',
        database: 'testdb',
      };

      const result = convertToPostgresBaseConfig(config);

      expect(result.connectionString).toBe(
        'postgresql://testuser:testpass@localhost:5432/testdb'
      );
      expect(result.poolConfig?.max).toBe(20);
      expect(result.poolConfig?.min).toBe(1);
      expect(result.useConnectionPool).toBe(true);
    });

    it('should handle custom port', () => {
      const config: PostgresStorageConfig = {
        host: 'localhost',
        port: 5433,
        username: 'testuser',
        password: 'testpass',
        database: 'testdb',
      };

      const result = convertToPostgresBaseConfig(config);

      expect(result.connectionString).toBe(
        'postgresql://testuser:testpass@localhost:5433/testdb'
      );
    });

    it('should enable SSL when configured', () => {
      const config: PostgresStorageConfig = {
        host: 'localhost',
        username: 'testuser',
        password: 'testpass',
        database: 'testdb',
        ssl: true,
      };

      const result = convertToPostgresBaseConfig(config);

      expect(result.connectionString).toBe(
        'postgresql://testuser:testpass@localhost:5432/testdb?sslmode=require'
      );
    });

    it('should handle custom pool settings', () => {
      const config: PostgresStorageConfig = {
        host: 'localhost',
        username: 'testuser',
        password: 'testpass',
        database: 'testdb',
        poolSize: 10,
        minConnections: 2,
        idleTimeout: 15000,
        connectionTimeout: 3000,
        maxUses: 1000,
      };

      const result = convertToPostgresBaseConfig(config);

      expect(result.poolConfig?.max).toBe(10);
      expect(result.poolConfig?.min).toBe(2);
      expect(result.poolConfig?.idleTimeoutMillis).toBe(15000);
      expect(result.poolConfig?.connectionTimeoutMillis).toBe(3000);
      expect(result.poolConfig?.maxUses).toBe(1000);
    });

    it('should encode special characters in credentials', () => {
      const config: PostgresStorageConfig = {
        host: 'localhost',
        username: 'user@domain',
        password: 'p@ss:word',
        database: 'my-db',
      };

      const result = convertToPostgresBaseConfig(config);

      expect(result.connectionString).toBe(
        'postgresql://user%40domain:p%40ss%3Aword@localhost:5432/my-db'
      );
    });
  });

  describe('convertToSqliteBaseConfig', () => {
    it('should convert basic SQLite config', () => {
      const config: SqliteStorageConfig = {
        dbPath: './data/test.db',
        enableWAL: true,
        enableLogging: false,
        readonly: false,
        fileMustExist: false,
        timeout: 5000,
      };

      const result = convertToSqliteBaseConfig(config);

      expect(result.dbPath).toBe('./data/test.db');
      expect(result.enableLogging).toBe(false);
      expect(result.readonly).toBe(false);
      expect(result.fileMustExist).toBe(false);
      expect(result.timeout).toBe(5000);
      expect(result.useConnectionPool).toBe(true);
    });

    it('should use default values for optional fields', () => {
      const config: SqliteStorageConfig = {
        dbPath: './data/test.db',
        enableWAL: true,
        enableLogging: false,
        readonly: false,
        fileMustExist: false,
        timeout: 5000,
      };

      const result = convertToSqliteBaseConfig(config);

      expect(result.useConnectionPool).toBe(true);
    });
  });
});
