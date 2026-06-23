/**
 * Cache Module
 * High-performance caching solution using xxHash (WebAssembly)
 * 90-140x faster hashing than traditional algorithms
 *
 * Supported hash algorithms:
 * - xxHash (32/64-bit): High-speed, non-cryptographic (for caching)
 * - SHA256: Cryptographic hash (for integrity verification)
 */

export { CacheManager, createCache } from './cache-manager.js';
export {
  XXHash32Algorithm,
  XXHash64Algorithm,
  StreamingXXHash64Algorithm,
  createHashAlgorithm,
} from './xxhash-algorithm.js';
export { SHA256Algorithm, createSHA256Algorithm } from './sha256-algorithm.js';
export type { CacheEntry, CacheStats, CacheConfig, IHashAlgorithm, ICache } from './types.js';
