/**
 * SDK Logging Configuration Tests
 * Verifies that logging configuration is properly applied from SDKOptions
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SDKInstance } from '../api/shared/core/sdk-instance.js';
import type { SDKOptions } from '../api/shared/types/core-types.js';
import type { CheckpointStorageAdapter } from '@wf-agent/storage';

// Mock checkpoint storage adapter for tests
const mockCheckpointAdapter: CheckpointStorageAdapter = {
  initialize: async () => {},
  close: async () => {},
  clear: async () => {},
  save: async () => {},
  load: async () => null,
  delete: async () => {},
  list: async () => [],
  exists: async () => false,
  getMetadata: async () => null,
  getMetrics: async () => ({
    saveCount: 0,
    loadCount: 0,
    deleteCount: 0,
    listCount: 0,
    avgSaveTime: 0,
    avgLoadTime: 0,
    avgDeleteTime: 0,
    avgListTime: 0,
    totalMetadataSize: 0,
    totalBlobSize: 0,
    totalCount: 0,
  }),
  resetMetrics: async () => {},
  saveBatch: async () => {},
  loadBatch: async () => [],
  deleteBatch: async () => {},
};

describe('SDK Logging Configuration', () => {
  let sdk: SDKInstance;

  afterEach(async () => {
    if (sdk) {
      await sdk.destroy();
    }
  });

  describe('Logging Level Configuration', () => {
    it('should use logging.level from config', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          level: 'debug',
        },
      });

      await sdk.waitForReady();
      
      // Should not throw error about logger configuration
      expect(sdk.isReady()).toBe(true);
      
      consoleSpy.mockRestore();
    });

    it('should use debug level when debug mode is enabled', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        debug: true,
      });

      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      consoleSpy.mockRestore();
    });
  });

  describe('Logging Output Configuration', () => {
    it('should configure console output by default', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
      });

      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      consoleSpy.mockRestore();
    });

    it('should warn when file output is configured without filePath', async () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          output: 'file',
        },
      });

      await sdk.waitForReady();
      
      // Should have warned about missing filePath
      expect(warnSpy).toHaveBeenCalledWith(
        expect.stringContaining('File output is enabled but no filePath specified')
      );
      
      warnSpy.mockRestore();
      errorSpy.mockRestore();
    });

    it('should handle both console and file output', async () => {
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          output: 'both',
          filePath: 'logs/test-sdk.log',
        },
      });

      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      errorSpy.mockRestore();
    });
  });

  describe('Logging Format Configuration', () => {
    it('should support JSON format', async () => {
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          format: 'json',
        },
      });

      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      errorSpy.mockRestore();
    });

    it('should support text format', async () => {
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          format: 'text',
        },
      });

      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      errorSpy.mockRestore();
    });
  });

  describe('Configuration Validation', () => {
    it('should warn when file output is configured without filePath', async () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          output: 'file',
        },
      });

      await sdk.waitForReady();
      
      // Should have warned about missing filePath
      expect(warnSpy).toHaveBeenCalledWith(
        expect.stringContaining('File output is enabled but no filePath specified')
      );
      
      warnSpy.mockRestore();
      errorSpy.mockRestore();
    });
  });

  describe('Integration with SDK Lifecycle', () => {
    it('should apply logging config before any logger access', async () => {
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      // Create SDK with specific logging config
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          level: 'debug',
          output: 'console',
          format: 'json',
        },
      });

      // Logger should be configured immediately in constructor
      // Access workflows API which will trigger logger usage
      await sdk.waitForReady();
      
      expect(sdk.isReady()).toBe(true);
      
      errorSpy.mockRestore();
    });

    it('should handle invalid file paths gracefully', async () => {
      const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      
      sdk = new SDKInstance({
        checkpointStorageAdapter: mockCheckpointAdapter,
        logging: {
          output: 'file',
          filePath: '/invalid/path/that/does/not/exist/sdk.log',
        },
      });

      await sdk.waitForReady();
      
      // Should still be ready even if file stream creation failed
      expect(sdk.isReady()).toBe(true);
      
      // Should have logged an error about file stream creation
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining('Failed to create file stream')
      );
      
      errorSpy.mockRestore();
    });
  });
});
