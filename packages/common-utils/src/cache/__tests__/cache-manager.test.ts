/**
 * Cache Manager Tests
 * Testing the xxHash-based cache implementation
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { CacheManager, createCache, createCacheSync } from '../cache-manager';
import type { CacheConfig } from '../../types';

describe('CacheManager', () => {
  describe('Async initialization', () => {
    let cache: CacheManager<string, unknown>;

    beforeEach(async () => {
      const config: CacheConfig = {
        maxSize: 100,
        enableStats: true,
      };
      cache = await createCache(config);
    });

    describe('Basic Operations', () => {
      it('should store and retrieve values', () => {
        cache.set('key1', { data: 'value1' });
        const result = cache.get('key1');
        expect(result).toEqual({ data: 'value1' });
      });

      it('should return null for missing keys', () => {
        const result = cache.get('nonexistent');
        expect(result).toBeNull();
      });

      it('should check key existence', () => {
        cache.set('key1', 'value1');
        expect(cache.has('key1')).toBe(true);
        expect(cache.has('nonexistent')).toBe(false);
      });

      it('should delete entries', () => {
        cache.set('key1', 'value1');
        expect(cache.has('key1')).toBe(true);
        cache.delete('key1');
        expect(cache.has('key1')).toBe(false);
      });

      it('should clear all entries', () => {
        cache.set('key1', 'value1');
        cache.set('key2', 'value2');
        expect(cache.size()).toBe(2);
        cache.clear();
        expect(cache.size()).toBe(0);
      });
    });

    describe('Size and Statistics', () => {
      it('should track cache size', () => {
        expect(cache.size()).toBe(0);
        cache.set('key1', 'value1');
        expect(cache.size()).toBe(1);
        cache.set('key2', 'value2');
        expect(cache.size()).toBe(2);
      });

      it('should track hits and misses', () => {
        cache.set('key1', 'value1');

        // Miss
        cache.get('nonexistent');

        // Hit
        cache.get('key1');
        cache.get('key1');

        const stats = cache.getStats();
        expect(stats.hits).toBe(2);
        expect(stats.misses).toBe(1);
        expect(stats.hitRate).toBeCloseTo(0.667, 2);
      });

      it('should track evictions', async () => {
        const smallCache = await createCache<string, string>({ maxSize: 2 });

        smallCache.set('key1', 'value1');
        smallCache.set('key2', 'value2');
        expect(smallCache.getStats().evictions).toBe(0);

        smallCache.set('key3', 'value3'); // Should evict key1
        expect(smallCache.getStats().evictions).toBe(1);
      });
    });

    describe('TTL (Time to Live)', () => {
      it('should expire entries based on TTL', (done) => {
        cache.set('key1', 'value1', 100); // 100ms TTL

        expect(cache.get('key1')).toEqual('value1');

        setTimeout(() => {
          expect(cache.get('key1')).toBeNull();
          done();
        }, 150);
      });

      it('should use default TTL if not specified', async (done) => {
        const configWithTTL: CacheConfig = {
          maxSize: 100,
          ttl: 100,
        };
        const cacheWithTTL = await createCache(configWithTTL);

        cacheWithTTL.set('key1', 'value1');
        expect(cacheWithTTL.get('key1')).toEqual('value1');

        setTimeout(() => {
          expect(cacheWithTTL.get('key1')).toBeNull();
          done();
        }, 150);
      });
    });

    describe('LRU Eviction', () => {
      it('should evict least recently used items', async () => {
        const smallCache = await createCache<string, string>({ maxSize: 3 });

        smallCache.set('a', '1');
        smallCache.set('b', '2');
        smallCache.set('c', '3');

        // Access 'a' to make it recently used
        smallCache.get('a');

        // Add new item - should evict 'b' (least recently used)
        smallCache.set('d', '4');

        expect(smallCache.has('a')).toBe(true);
        expect(smallCache.has('b')).toBe(false);
        expect(smallCache.has('c')).toBe(true);
        expect(smallCache.has('d')).toBe(true);
      });
    });

    describe('Hash Algorithm (xxHash)', () => {
      it('should generate consistent hash keys', () => {
        const key = { complex: 'object', with: ['nested', 'data'] };
        const hash1 = cache.getHashKey(key);
        const hash2 = cache.getHashKey(key);

        expect(hash1).toBe(hash2);
      });

      it('should hash different inputs to different keys', () => {
        const hash1 = cache.getHashKey('input1');
        const hash2 = cache.getHashKey('input2');

        expect(hash1).not.toBe(hash2);
      });

      it('should generate hex hash strings', () => {
        const shortString = 'test';
        const hash = cache.getHashKey(shortString);

        // xxHash returns hex string
        expect(hash).toMatch(/^[0-9a-f]{16}$/);
      });
    });

    describe('Complex Values', () => {
      it('should store and retrieve complex objects', () => {
        const complexValue = {
          nested: {
            deep: {
              data: [1, 2, 3],
              flag: true,
            },
          },
          name: 'test',
        };

        cache.set('complex', complexValue);
        const retrieved = cache.get('complex');

        expect(retrieved).toEqual(complexValue);
      });

      it('should store arrays', () => {
        const array = [1, 'two', { three: 3 }, true, null];
        cache.set('array', array);
        expect(cache.get('array')).toEqual(array);
      });

      it('should store primitives', () => {
        cache.set('number', 42);
        cache.set('string', 'hello');
        cache.set('boolean', true);
        cache.set('null', null);

        expect(cache.get('number')).toBe(42);
        expect(cache.get('string')).toBe('hello');
        expect(cache.get('boolean')).toBe(true);
        expect(cache.get('null')).toBe(null);
      });
    });
  });

  describe('Sync creation with manual initialization', () => {
    let cache: CacheManager<string, unknown>;

    beforeEach(async () => {
      const config: CacheConfig = {
        maxSize: 100,
        enableStats: true,
      };
      cache = createCacheSync(config);
      await cache.initialize();
    });

    it('should work with manual initialization', () => {
      cache.set('key1', 'value1');
      expect(cache.get('key1')).toBe('value1');
    });

    it('should throw error if not initialized', () => {
      const uninitializedCache = createCacheSync<string, string>({ maxSize: 100 });
      expect(() => uninitializedCache.set('key', 'value')).toThrow();
    });
  });

  describe('Hash bits configuration', () => {
    it('should use xxHash32 when hashBits=32', async () => {
      const cache = await createCache<string, unknown>({
        maxSize: 100,
        hashBits: 32,
      });

      cache.set('key1', 'value1');
      const hash = cache.getHashKey('key1');

      // xxHash32 produces shorter hashes
      expect(hash).toMatch(/^[0-9a-f]{8}$/);
    });

    it('should use xxHash64 when hashBits=64', async () => {
      const cache = await createCache<string, unknown>({
        maxSize: 100,
        hashBits: 64,
      });

      cache.set('key1', 'value1');
      const hash = cache.getHashKey('key1');

      // xxHash64 produces 16-char hex strings
      expect(hash).toMatch(/^[0-9a-f]{16}$/);
    });

    it('should default to xxHash64', async () => {
      const cache = await createCache<string, unknown>({
        maxSize: 100,
      });

      const hash = cache.getHashKey('key1');
      expect(hash).toMatch(/^[0-9a-f]{16}$/);
    });
  });
});
