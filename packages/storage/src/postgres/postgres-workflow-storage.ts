/**
 * PostgreSQL Workflow Storage Implementation with Metadata-BLOB Separation
 * Workflow persistence storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 * - Version control with change notes
 */

import type { 
  WorkflowStorageMetadata, 
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
} from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "../types/adapter/index.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-workflow-storage");

export class PostgresWorkflowStorage
  extends BasePostgresStorage<WorkflowStorageMetadata>
  implements WorkflowStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "workflow_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "workflow_blob";
  }

  /**
   * Get version metadata table name
   */
  protected getVersionMetadataTableName(): string {
    return "workflow_version_metadata";
  }

  /**
   * Get version BLOB table name
   */
  protected getVersionBlobTableName(): string {
    return "workflow_version_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Workflow metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS workflow_metadata (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        description TEXT,
        author TEXT,
        category TEXT,
        tags JSONB,
        enabled BOOLEAN DEFAULT TRUE,
        node_count INTEGER NOT NULL,
        edge_count INTEGER NOT NULL,
        blob_size INTEGER,
        blob_hash TEXT,
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        custom_fields JSONB
      )
    `);

    // Layer 2: Workflow BLOB storage table (infrequent direct access)
    await client.query(`
      CREATE TABLE IF NOT EXISTS workflow_blob (
        workflow_id TEXT PRIMARY KEY REFERENCES workflow_metadata(id) ON DELETE CASCADE,
        blob_data BYTEA NOT NULL,
        compressed BOOLEAN DEFAULT FALSE,
        compression_algorithm TEXT,
        CHECK (
          (compressed = FALSE AND compression_algorithm IS NULL) OR
          (compressed = TRUE AND compression_algorithm IS NOT NULL)
        )
      )
    `);

    // Layer 3: Version metadata table
    await client.query(`
      CREATE TABLE IF NOT EXISTS workflow_version_metadata (
        id SERIAL PRIMARY KEY,
        workflow_id TEXT NOT NULL REFERENCES workflow_metadata(id) ON DELETE CASCADE,
        version TEXT NOT NULL,
        change_note TEXT,
        blob_size INTEGER,
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        created_by TEXT,
        UNIQUE(workflow_id, version)
      )
    `);

    // Layer 4: Version BLOB storage table
    await client.query(`
      CREATE TABLE IF NOT EXISTS workflow_version_blob (
        version_id INTEGER PRIMARY KEY REFERENCES workflow_version_metadata(id) ON DELETE CASCADE,
        blob_data BYTEA NOT NULL,
        compressed BOOLEAN DEFAULT FALSE,
        compression_algorithm TEXT,
        CHECK (
          (compressed = FALSE AND compression_algorithm IS NULL) OR
          (compressed = TRUE AND compression_algorithm IS NOT NULL)
        )
      )
    `);

    // Create indexes for optimized queries
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_name ON workflow_metadata(name)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_category ON workflow_metadata(category)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_author ON workflow_metadata(author)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_enabled ON workflow_metadata(enabled)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_created_at ON workflow_metadata(created_at)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_meta_updated_at ON workflow_metadata(updated_at)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_ver_meta_workflow_id ON workflow_version_metadata(workflow_id)'
    );

    logger.debug('Workflow schema created');
  }

  /**
   * Save workflow with metadata-BLOB separation and compression
   */
  async save(
    workflowId: string,
    data: Uint8Array,
    metadata: WorkflowStorageMetadata
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, workflowId, data, metadata);
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Workflow saved', {
          workflowId,
          dataSize: data.length,
        });
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'save', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client with compression
   */
  protected async saveToClient(
    client: PoolClient,
    workflowId: string,
    data: Uint8Array,
    metadata: WorkflowStorageMetadata
  ): Promise<void> {
    const now = Date.now();

    // Get compression config
    const config = selectCompressionStrategy(data);

    // Compress BLOB data
    const { compressed, algorithm } = await compressBlob(data, config);
    
    // Compute blob hash
    const blobHash = await this.computeHash(data);

    // Insert or update metadata
    await client.query(
      `INSERT INTO workflow_metadata (
        id, name, version, description, author, category, tags, enabled,
        node_count, edge_count, blob_size, blob_hash, created_at, updated_at, custom_fields
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW(), $13)
      ON CONFLICT (id) DO UPDATE SET
        name = EXCLUDED.name,
        version = EXCLUDED.version,
        description = EXCLUDED.description,
        author = EXCLUDED.author,
        category = EXCLUDED.category,
        tags = EXCLUDED.tags,
        enabled = EXCLUDED.enabled,
        node_count = EXCLUDED.node_count,
        edge_count = EXCLUDED.edge_count,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        updated_at = NOW(),
        custom_fields = EXCLUDED.custom_fields`,
      [
        workflowId,
        metadata.name,
        metadata.version,
        metadata.description ?? null,
        metadata.author ?? null,
        metadata.category ?? null,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.enabled !== false,
        metadata.nodeCount,
        metadata.edgeCount,
        compressed.length,
        blobHash,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO workflow_blob (workflow_id, blob_data, compressed, compression_algorithm)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (workflow_id) DO UPDATE SET
         blob_data = EXCLUDED.blob_data,
         compressed = EXCLUDED.compressed,
         compression_algorithm = EXCLUDED.compression_algorithm`,
      [
        workflowId,
        Buffer.from(compressed),
        algorithm !== null,
        algorithm,
      ]
    );
  }

  /**
   * Load workflow data with automatic decompression
   */
  async load(workflowId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const result = await client.query(
        `SELECT wb.blob_data, wb.compressed, wb.compression_algorithm
         FROM workflow_blob wb
         WHERE wb.workflow_id = $1`,
        [workflowId]
      );

      if (result.rows.length === 0) {
        logger.debug('Workflow not found', { workflowId });
        return null;
      }

      const row = result.rows[0];
      let data = row.blob_data;

      // Decompress if needed
      if (row.compressed && row.compression_algorithm) {
        data = await decompressBlob(data, row.compression_algorithm);
      }

      // Verify integrity if enabled
      if (this.shouldVerifyIntegrity()) {
        const metaResult = await client.query(
          'SELECT blob_hash FROM workflow_metadata WHERE id = $1',
          [workflowId]
        );
        if (metaResult.rows.length > 0) {
          await this.verifyIntegrity(data, metaResult.rows[0].blob_hash, workflowId);
        }
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Workflow loaded', {
        workflowId,
        dataSize: data.length,
        compressed: row.compressed,
      });

      return data;
    } catch (error) {
      return this.handlePostgresError(error, 'load', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete workflow (cascade delete will handle blobs and versions)
   */
  async delete(workflowId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, workflowId);
      logger.debug('Workflow deleted', { workflowId });
    } catch (error) {
      return this.handlePostgresError(error, 'delete', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client
   */
  protected async deleteFromClient(client: PoolClient, workflowId: string): Promise<void> {
    // Delete from metadata table (cascade will delete blob and versions)
    await client.query(
      'DELETE FROM workflow_metadata WHERE id = $1',
      [workflowId]
    );
  }

  /**
   * List workflow IDs (optimized - only scans metadata table)
   */
  async list(options?: WorkflowListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: any[] = [];
      let paramIndex = 1;

      if (options?.name) {
        conditions.push(`name ILIKE $${paramIndex++}`);
        params.push(`%${options.name}%`);
      }

      if (options?.author) {
        conditions.push(`author = $${paramIndex++}`);
        params.push(options.author);
      }

      if (options?.category) {
        conditions.push(`category = $${paramIndex++}`);
        params.push(options.category);
      }

      if (options?.enabled !== undefined) {
        conditions.push(`enabled = $${paramIndex++}`);
        params.push(options.enabled);
      }

      if (options?.createdAtFrom) {
        conditions.push(`created_at >= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.createdAtFrom / 1000);
      }

      if (options?.createdAtTo) {
        conditions.push(`created_at <= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.createdAtTo / 1000);
      }

      if (options?.updatedAtFrom) {
        conditions.push(`updated_at >= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.updatedAtFrom / 1000);
      }

      if (options?.updatedAtTo) {
        conditions.push(`updated_at <= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.updatedAtTo / 1000);
      }

      if (options?.tags && options.tags.length > 0) {
        // Check if tags array contains any of the specified tags
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      // Sort by validated column
      const sortBy = options?.sortBy ?? 'updatedAt';
      const sortOrder = options?.sortOrder ?? 'desc';
      
      const sortColumnMap: Record<string, string> = {
        updatedAt: 'updated_at',
        createdAt: 'created_at',
        name: 'name',
      };
      
      const sortColumn = sortColumnMap[sortBy] || 'updated_at';
      const orderDirection = sortOrder.toLowerCase() === 'asc' ? 'ASC' : 'DESC';

      const query = `
        SELECT id FROM workflow_metadata
        ${whereClause}
        ORDER BY ${sortColumn} ${orderDirection}
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Workflows listed', {
        count: ids.length,
        filters: Object.keys(options || {}),
      });

      return ids;
    } catch (error) {
      return this.handlePostgresError(error, 'list', { options });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Check if workflow exists (only check metadata table)
   */
  async exists(workflowId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM workflow_metadata WHERE id = $1',
        [workflowId]
      );
      return result.rows.length > 0;
    } catch (error) {
      return this.handlePostgresError(error, 'exists', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get metadata (optimized - only reads metadata table)
   */
  async getMetadata(workflowId: string): Promise<WorkflowStorageMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM workflow_metadata WHERE id = $1`,
        [workflowId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      // Parse JSON fields and convert timestamps
      const metadata: WorkflowStorageMetadata = {
        workflowId: row.id,
        name: row.name,
        version: row.version,
        description: row.description ?? undefined,
        author: row.author ?? undefined,
        category: row.category ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        enabled: row.enabled,
        nodeCount: row.node_count,
        edgeCount: row.edge_count,
        createdAt: new Date(row.created_at).getTime(),
        updatedAt: new Date(row.updated_at).getTime(),
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      return this.handlePostgresError(error, 'getMetadata', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Update workflow metadata (optimized - only updates metadata table)
   */
  async updateWorkflowMetadata(
    workflowId: string,
    metadata: Partial<WorkflowStorageMetadata>
  ): Promise<void> {
    const client = await this.getClient();

    try {
      const updates: string[] = ['updated_at = NOW()'];
      const params: any[] = [];
      let paramIndex = 1;

      if (metadata.name !== undefined) {
        updates.push(`name = $${paramIndex++}`);
        params.push(metadata.name);
      }
      if (metadata.description !== undefined) {
        updates.push(`description = $${paramIndex++}`);
        params.push(metadata.description);
      }
      if (metadata.author !== undefined) {
        updates.push(`author = $${paramIndex++}`);
        params.push(metadata.author);
      }
      if (metadata.category !== undefined) {
        updates.push(`category = $${paramIndex++}`);
        params.push(metadata.category);
      }
      if (metadata.tags !== undefined) {
        updates.push(`tags = $${paramIndex++}`);
        params.push(JSON.stringify(metadata.tags));
      }
      if (metadata.enabled !== undefined) {
        updates.push(`enabled = $${paramIndex++}`);
        params.push(metadata.enabled);
      }
      if (metadata.customFields !== undefined) {
        updates.push(`custom_fields = $${paramIndex++}`);
        params.push(JSON.stringify(metadata.customFields));
      }

      params.push(workflowId);

      await client.query(
        `UPDATE workflow_metadata SET ${updates.join(', ')} WHERE id = $${paramIndex}`,
        params
      );

      logger.debug('Workflow metadata updated', { workflowId });
    } catch (error) {
      return this.handlePostgresError(error, 'updateWorkflowMetadata', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  // ==================== Version Control ====================

  /**
   * Save workflow version with metadata-BLOB separation
   */
  async saveWorkflowVersion(
    workflowId: string,
    version: string,
    data: Uint8Array,
    changeNote?: string
  ): Promise<void> {
    const client = await this.getClient();

    try {
      await client.query('BEGIN');

      try {
        // Get compression config
        const config = selectCompressionStrategy(data);

        // Compress BLOB data
        const { compressed, algorithm } = await compressBlob(data, config);

        // Insert version metadata and get the version_id
        const metaResult = await client.query(
          `INSERT INTO workflow_version_metadata (
            workflow_id, version, change_note, blob_size, created_at
          )
          VALUES ($1, $2, $3, $4, NOW())
          ON CONFLICT (workflow_id, version) DO UPDATE SET
            change_note = EXCLUDED.change_note,
            blob_size = EXCLUDED.blob_size,
            created_at = NOW()
          RETURNING id`,
          [workflowId, version, changeNote ?? null, compressed.length]
        );

        if (metaResult.rows.length === 0) {
          throw new Error('Failed to insert version metadata');
        }

        const versionId = metaResult.rows[0].id;

        // Insert version blob
        await client.query(
          `INSERT INTO workflow_version_blob (version_id, blob_data, compressed, compression_algorithm)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (version_id) DO UPDATE SET
             blob_data = EXCLUDED.blob_data,
             compressed = EXCLUDED.compressed,
             compression_algorithm = EXCLUDED.compression_algorithm`,
          [
            versionId,
            Buffer.from(compressed),
            algorithm !== null,
            algorithm,
          ]
        );

        await client.query('COMMIT');

        logger.debug('Workflow version saved', { workflowId, version });
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'saveWorkflowVersion', { workflowId, version });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * List workflow versions (optimized - only reads metadata table)
   */
  async listWorkflowVersions(
    workflowId: string,
    options?: WorkflowVersionListOptions
  ): Promise<WorkflowVersionInfo[]> {
    const client = await this.getClient();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Get versions
      const versionResult = await client.query(
        `SELECT v.id, v.version, v.created_at, v.created_by, v.change_note
         FROM workflow_version_metadata v
         WHERE v.workflow_id = $1
         ORDER BY v.created_at DESC
         LIMIT $2 OFFSET $3`,
        [workflowId, limit, offset]
      );

      // Get current version
      const currentResult = await client.query(
        'SELECT version FROM workflow_metadata WHERE id = $1',
        [workflowId]
      );
      const currentVersion = currentResult.rows.length > 0 ? currentResult.rows[0].version : null;

      const versions: WorkflowVersionInfo[] = versionResult.rows.map(row => ({
        version: row.version,
        createdAt: new Date(row.created_at).getTime(),
        createdBy: row.created_by ?? undefined,
        changeNote: row.change_note ?? undefined,
        isCurrent: row.version === currentVersion,
      }));

      logger.debug('Workflow versions listed', { workflowId, count: versions.length });

      return versions;
    } catch (error) {
      return this.handlePostgresError(error, 'listWorkflowVersions', { workflowId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Load workflow version with automatic decompression
   */
  async loadWorkflowVersion(workflowId: string, version: string): Promise<Uint8Array | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT vb.blob_data, vb.compressed, vb.compression_algorithm
         FROM workflow_version_blob vb
         JOIN workflow_version_metadata vm ON vb.version_id = vm.id
         WHERE vm.workflow_id = $1 AND vm.version = $2`,
        [workflowId, version]
      );

      if (result.rows.length === 0) {
        logger.debug('Workflow version not found', { workflowId, version });
        return null;
      }

      const row = result.rows[0];
      let data = row.blob_data;

      // Decompress if needed
      if (row.compressed && row.compression_algorithm) {
        data = await decompressBlob(data, row.compression_algorithm);
      }

      logger.debug('Workflow version loaded', { workflowId, version });

      return data;
    } catch (error) {
      return this.handlePostgresError(error, 'loadWorkflowVersion', { workflowId, version });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete workflow version
   */
  async deleteWorkflowVersion(workflowId: string, version: string): Promise<void> {
    const client = await this.getClient();

    try {
      // Cascade delete will handle blob table
      await client.query(
        `DELETE FROM workflow_version_metadata 
         WHERE workflow_id = $1 AND version = $2`,
        [workflowId, version]
      );

      logger.debug('Workflow version deleted', { workflowId, version });
    } catch (error) {
      return this.handlePostgresError(error, 'deleteWorkflowVersion', { workflowId, version });
    } finally {
      this.releaseClient(client);
    }
  }
}
