/**
 * Keyboard input handling for terminal applications.
 * Supports both legacy terminal sequences and Kitty keyboard protocol.
 * 
 * API:
 * - matchesKey(data, keyId) - Check if input matches a key identifier
 * - parseKey(data) - Parse input and return the key identifier
 * - Key - Helper object for creating typed key identifiers
 * - setKittyProtocolActive(active) - Set global Kitty protocol state
 * - isKittyProtocolActive() - Query global Kitty protocol state
 */

// Export types
export type { KeyId, KeyEventType } from "./types.js";
export { Key } from "./types.js";

// Export Kitty protocol state management
export { setKittyProtocolActive, isKittyProtocolActive } from "./kitty-protocol.js";

// Export event type queries
export { isKeyRelease, isKeyRepeat } from "./kitty-protocol.js";

// Export core matching function
export { matchesKey } from "./matching.js";

// Export parsing functions
export { parseKey } from "./parsing.js";

// Export printable character decoding
export { decodeKittyPrintable, decodePrintableKey } from "./parsing.js";
