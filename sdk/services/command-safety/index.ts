/**
 * Command Safety — Shared Utilities
 *
 * Provides command chain parsing, dangerous pattern detection, and
 * allowlist/denylist matching shared between auto-approval (pre-execution
 * decision layer) and sandbox (runtime enforcement layer).
 */

export { parseCommandChain } from "./command-chain-parser.js";
export {
  containsDangerousSubstitution,
  findLongestPrefixMatch,
  getCommandDecision,
  getSingleCommandDecision,
  type CommandDecision,
} from "./command-safety-checker.js";
