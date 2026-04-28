/**
 * Command Safety Checker
 * Detects dangerous command patterns and implements longest prefix match for allowlist/denylist
 */

/**
 * Detect dangerous parameter substitutions that could lead to command execution.
 * These patterns are never auto-approved and always require explicit user approval.
 *
 * Detected patterns:
 * - ${var@P} - Prompt string expansion (interprets escape sequences and executes embedded commands)
 * - ${var@Q} - Quote removal
 * - ${var@E} - Escape sequence expansion
 * - ${var@A} - Assignment statement
 * - ${var@a} - Attribute flags
 * - ${var=value} with escape sequences - Can embed commands via \140 (backtick), \x60, or \u0060
 * - ${!var} - Indirect variable references
 * - <<<$(...) or <<<`...` - Here-strings with command substitution
 * - =(...) - Zsh process substitution that executes commands
 * - *(e:...:) or similar - Zsh glob qualifiers with code execution
 *
 * @param command - The command string to analyze
 * @returns true if dangerous substitution patterns are detected, false otherwise
 */
export function containsDangerousSubstitution(command: string): boolean {
  // Check for dangerous parameter expansion operators
  // ${var@P} - Prompt string expansion (interprets escape sequences and executes embedded commands)
  // ${var@Q} - Quote removal
  // ${var@E} - Escape sequence expansion
  // ${var@A} - Assignment statement
  // ${var@a} - Attribute flags
  const dangerousParameterExpansion = /\$\{[^}]*@[PQEAa][^}]*\}/.test(command);

  // Check for parameter expansions with assignments that could contain escape sequences
  // ${var=value} or ${var:=value} can embed commands via escape sequences like \140 (backtick)
  const parameterAssignmentWithEscapes =
    /\$\{[^}]*[=+\-?][^}]*\\[0-7]{3}[^}]*\}/.test(command) || // octal escapes
    /\$\{[^}]*[=+\-?][^}]*\\x[0-9a-fA-F]{2}[^}]*\}/.test(command) || // hex escapes
    /\$\{[^}]*[=+\-?][^}]*\\u[0-9a-fA-F]{4}[^}]*\}/.test(command); // unicode escapes

  // Check for indirect variable references
  // ${!var} performs indirect expansion which can be dangerous
  const indirectExpansion = /\$\{![^}]+\}/.test(command);

  // Check for here-strings with command substitution
  // <<<$(...) or <<<`...` can execute commands
  const hereStringWithSubstitution = /<<<\s*(\$\(|`)/.test(command);

  // Check for zsh process substitution =(...) which executes commands
  const zshProcessSubstitution = /(?:(?<=^)|(?<=[\s;|&(<]))=\([^)]+\)/.test(command);

  // Check for zsh glob qualifiers with code execution (e:...:)
  // Patterns like *(e:whoami:) or ?(e:rm -rf /:) execute commands during glob expansion
  const zshGlobQualifier = /[*?+@!]\(e:[^:]+:\)/.test(command);

  return (
    dangerousParameterExpansion ||
    parameterAssignmentWithEscapes ||
    indirectExpansion ||
    hereStringWithSubstitution ||
    zshProcessSubstitution ||
    zshGlobQualifier
  );
}

/**
 * Find the longest matching prefix from a list of prefixes for a given command.
 *
 * This implements the "longest prefix match" strategy for resolving conflicts
 * between allowlist and denylist patterns.
 *
 * Special Cases:
 * - Wildcard "*" matches any command but is treated as length 1 for comparison
 * - Empty command or empty prefixes list returns null
 * - Matching is case-insensitive and uses startsWith logic
 *
 * @param command - The command to match against
 * @param prefixes - List of prefix patterns to search through
 * @returns The longest matching prefix, or null if no match found
 */
export function findLongestPrefixMatch(command: string, prefixes: string[]): string | null {
  if (!command || !prefixes?.length) {
    return null;
  }

  const trimmedCommand = command.trim().toLowerCase();
  let longestMatch: string | null = null;

  for (const prefix of prefixes) {
    const lowerPrefix = prefix.toLowerCase();
    // Handle wildcard "*" - it matches any command
    if (lowerPrefix === "*" || trimmedCommand.startsWith(lowerPrefix)) {
      if (!longestMatch || lowerPrefix.length > longestMatch.length) {
        longestMatch = lowerPrefix;
      }
    }
  }

  return longestMatch;
}

/**
 * Command approval decision types
 */
export type CommandDecision = "auto_approve" | "auto_deny" | "ask_user";

/**
 * Unified command validation that implements the longest prefix match rule.
 * Returns a definitive decision for a command based on allowlist and denylist.
 *
 * Decision Logic:
 * 1. Dangerous Substitution Protection: Commands with dangerous patterns are never auto-approved
 * 2. Command Parsing: Split command chains (&&, ||, ;, |, &) into individual commands
 * 3. Individual Validation: For each sub-command, apply longest prefix match rule
 * 4. Aggregation: Combine decisions using "any denial blocks all" principle
 *
 * @param command - The full command string to validate
 * @param allowedCommands - List of allowed command prefixes
 * @param deniedCommands - Optional list of denied command prefixes
 * @returns Decision indicating whether to approve, deny, or ask user
 */
export function getCommandDecision(
  command: string,
  allowedCommands: string[],
  deniedCommands?: string[]
): CommandDecision {
  if (!command?.trim()) {
    return "auto_approve";
  }

  // Check for dangerous patterns first - never auto-approve
  if (containsDangerousSubstitution(command)) {
    return "ask_user";
  }

  // Parse into sub-commands (split by &&, ||, ;, |, &)
  const subCommands = parseCommandChain(command);

  // Check each sub-command and collect decisions
  const decisions: CommandDecision[] = subCommands.map((cmd) => {
    // Remove simple PowerShell-like redirections (e.g. 2>&1) before checking
    const cmdWithoutRedirection = cmd.replace(/\d*>&\d*/, "").trim();
    return getSingleCommandDecision(cmdWithoutRedirection, allowedCommands, deniedCommands);
  });

  // If any sub-command is denied, deny the whole command
  if (decisions.includes("auto_deny")) {
    return "auto_deny";
  }

  // If all sub-commands are approved, approve the whole command
  if (decisions.every((decision) => decision === "auto_approve")) {
    return "auto_approve";
  }

  // Otherwise, ask user
  return "ask_user";
}

/**
 * Get the decision for a single command using longest prefix match rule.
 *
 * Decision Matrix:
 * | Allowlist Match | Denylist Match | Result | Reason |
 * |----------------|----------------|---------|---------|
 * | Yes | No | auto_approve | Only allowlist matches |
 * | No | Yes | auto_deny | Only denylist matches |
 * | Yes | Yes (shorter) | auto_approve | Allowlist is more specific |
 * | Yes | Yes (longer/equal) | auto_deny | Denylist is more specific |
 * | No | No | ask_user | No rules apply |
 *
 * @param command - Single command to validate (no chaining)
 * @param allowedCommands - List of allowed command prefixes
 * @param deniedCommands - Optional list of denied command prefixes
 * @returns Decision for this specific command
 */
export function getSingleCommandDecision(
  command: string,
  allowedCommands: string[],
  deniedCommands?: string[]
): CommandDecision {
  if (!command) return "auto_approve";

  // If no allowlist configured, nothing can be auto-approved
  if (!allowedCommands?.length) {
    return "ask_user";
  }

  // Check if wildcard is present in allowlist
  const hasWildcard = allowedCommands.some((cmd) => cmd.toLowerCase() === "*");

  // If no denylist provided (undefined), use simple allowlist logic
  if (deniedCommands === undefined) {
    const trimmedCommand = command.trim().toLowerCase();

    const hasMatch = allowedCommands.some((prefix) => {
      const lowerPrefix = prefix.toLowerCase();
      return lowerPrefix === "*" || trimmedCommand.startsWith(lowerPrefix);
    });

    return hasMatch ? "auto_approve" : "ask_user";
  }

  // Find longest matching prefixes in both lists
  const longestDeniedMatch = findLongestPrefixMatch(command, deniedCommands);
  const longestAllowedMatch = findLongestPrefixMatch(command, allowedCommands);

  // Special case: if wildcard is present and no denylist match, auto-approve
  if (hasWildcard && !longestDeniedMatch) {
    return "auto_approve";
  }

  // Must have an allowlist match to be auto-approved
  if (!longestAllowedMatch) {
    // If denylist matches without allowlist, deny
    if (longestDeniedMatch) {
      return "auto_deny";
    }
    return "ask_user";
  }

  // If no denylist match, auto-approve
  if (!longestDeniedMatch) {
    return "auto_approve";
  }

  // Both have matches - allowlist must be longer to auto-approve
  return longestAllowedMatch.length > longestDeniedMatch.length ? "auto_approve" : "auto_deny";
}

/**
 * Parse command chain into individual commands
 * Splits by &&, ||, ;, |, &
 *
 * @param command - The full command string
 * @returns Array of individual commands
 */
function parseCommandChain(command: string): string[] {
  // Simple implementation: split by common chain operators
  // This is a basic implementation; a more robust parser could be used
  const operators = ["&&", "||", ";", "|"];

  let result = [command];

  for (const op of operators) {
    result = result.flatMap((cmd) => cmd.split(op).map((c) => c.trim()));
  }

  return result.filter((cmd) => cmd.length > 0);
}
