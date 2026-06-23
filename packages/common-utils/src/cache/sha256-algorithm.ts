/**
 * SHA256 Hash Algorithm Implementation
 * Provides cryptographic hashing for integrity verification
 * Used for checkpoint validation, data fingerprinting, etc.
 */

import type { IHashAlgorithm } from './types.js';
import { createHash } from 'crypto';

/**
 * SHA256 Algorithm Implementation
 *
 * Characteristics:
 * - Cryptographically secure hash function
 * - 256-bit output (64 hex characters)
 * - Performance: ~100K ops/sec (depends on input size)
 * - Ideal for integrity verification, checksums, fingerprinting
 * - NOT suitable for high-frequency caching (use xxHash instead)
 *
 * Use cases:
 * - Checkpoint integrity verification
 * - Message fingerprinting
 * - Data integrity validation
 * - Cryptographic operations
 *
 * WARNING: This is a synchronous implementation that relies on Node.js crypto module.
 * Not suitable for browser environments or Web Workers.
 */
export class SHA256Algorithm implements IHashAlgorithm {
  readonly name = 'SHA256';

  /**
   * SHA256 does not require initialization (uses Node.js built-in crypto)
   */
  async initialize(): Promise<void> {
    // SHA256 uses Node.js built-in crypto, no initialization needed
  }

  /**
   * Generate SHA256 hash of input
   * @param input Data to hash (string, object, or any serializable value)
   * @returns 64-character hex string representing the hash
   */
  hash(input: unknown): string {
    try {
      const str = typeof input === 'string' ? input : JSON.stringify(input);
      return createHash('sha256').update(str).digest('hex');
    } catch (error) {
      throw new Error(`SHA256 hash failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * SHA256 is always initialized (uses Node.js built-in)
   */
  isInitialized(): boolean {
    return true;
  }
}

/**
 * Create a SHA256 algorithm instance
 * @returns Initialized SHA256 algorithm instance
 */
export function createSHA256Algorithm(): SHA256Algorithm {
  return new SHA256Algorithm();
}
