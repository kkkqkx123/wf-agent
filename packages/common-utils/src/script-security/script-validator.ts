/**
 * Script Validation Tool
 * Provides pure utility functions without any predefined validation logic.
 *
 * Note: The SDK relies entirely on user configuration and does not preset any validation rules. Custom validation logic should be implemented at the application layer according to actual requirements.
 *
 */

/**
 * Check if a string contains any of the specified patterns.
 * @param text The text to be checked
 * @param patterns An array of patterns to match
 * @returns Whether any of the patterns are contained
 */
export function containsAnyPattern(text: string, patterns: string[]): boolean {
  return patterns.some(pattern => text.includes(pattern));
}

/**
 * Check if a string matches any of the specified regular expressions.
 * @param text: The text to be checked
 * @param regexes: An array of regular expressions
 * @returns: Whether any of the regular expressions is matched
 */
export function matchesAnyRegex(text: string, regexes: RegExp[]): boolean {
  return regexes.some(regex => regex.test(text));
}

/**
 * Check if a string is in the whitelist.
 * @param text: The text to be checked
 * @param whitelist: The array of allowed (whitelist) strings
 * @returns: A boolean indicating whether the string is in the whitelist or not
 */
export function isInWhitelist(text: string, whitelist: string[]): boolean {
  return whitelist.includes(text);
}

/**
 * Check if a string is in the blacklist.
 * @param text The text to be checked
 * @param blacklist The array of blacklisted items
 * @returns Whether the string is in the blacklist
 */
export function isInBlacklist(text: string, blacklist: string[]): boolean {
  return blacklist.includes(text);
}
