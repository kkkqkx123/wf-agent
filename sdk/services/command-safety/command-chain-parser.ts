/**
 * Command Chain Parser
 *
 * Parses compound shell commands into individual sub-commands by splitting
 * on chain operators (&&, ||, ;, |).
 *
 * Shared by:
 *   - command-safety-checker.ts — pre-execution approval decisions
 *   - sandbox/strategies/shell-static-analyzer.ts — runtime sandbox analysis
 *
 * Without chain parsing, commands like `git checkout main && rm -rf /` would
 * only be checked on the first segment ("git checkout main"), letting the
 * destructive second segment bypass allowlist/denylist analysis.
 */

/**
 * Parse a command chain into individual sub-commands.
 *
 * Splits by common chain operators in order:
 *   1. &&  — AND
 *   2. ||  — OR
 *   3. ;   — Sequential
 *   4. |   — Pipe
 *   5. &   — Background
 *
 * Each sub-command is trimmed. Empty segments are filtered out.
 *
 * NOTE: This is a basic tokenizer, not a full shell parser. Quoted strings
 * containing operators (e.g. echo "a && b") are not handled — the operator
 * inside quotes will also split. For the static analysis use case this is
 * acceptable: false positives (splitting a safe echo) are harmless, while
 * false negatives (missing a real chain) are the greater security risk.
 *
 * @param command - The full command string
 * @returns Array of individual sub-commands
 */
export function parseCommandChain(command: string): string[] {
  const operators = ["&&", "||", ";", "|", "&"];

  let result = [command];

  for (const op of operators) {
    result = result.flatMap((cmd) => cmd.split(op).map((c) => c.trim()));
  }

  return result.filter((cmd) => cmd.length > 0);
}
