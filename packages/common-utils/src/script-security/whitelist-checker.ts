/**
 * Whitelist/blacklist checking tool
 * Provides pure tool functions without any predefined validation logic
 *
 * Note: SDK completely trusts user configuration and does not preset any whitelist or blacklist rules.
 * Application layer should implement customized whitelist/blacklist logic according to actual requirements.
 */

/**
 * Check if a string is in the whitelist
 * @param text The text to check
 * @param whitelist An array of whitelists.
 * @returns Whether the string is in the whitelist
 */
export function isInWhitelist(text: string, whitelist: string[]): boolean {
  return whitelist.includes(text);
}

/**
 * Check if a string is in the blacklist
 * @param text The text to check
 * @param blacklist An array of blacklists
 * @returns whether the string is in the blacklist
 */
export function isInBlacklist(text: string, blacklist: string[]): boolean {
  return blacklist.includes(text);
}

/**
 * Check if a string matches any of the patterns in the whitelist (wildcards are supported)
 * @param text The text to check
 * @param whitelistPatterns An array of whitelist patterns (with * wildcard support).
 * @returns if any of the whitelist patterns match @param whitelistPatterns
 */
export function matchesWhitelistPattern(text: string, whitelistPatterns: string[]): boolean {
  return whitelistPatterns.some(pattern => {
    const regexPattern = pattern.replace(/\*/g, ".*").replace(/\?/g, ".");
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(text);
  });
}

/**
 * Check if a string matches any of the patterns in the blacklist (wildcards are supported)
 * @param text The text to check
 * @param blacklistPatterns array of blacklist patterns (* wildcards supported)
 * @returns Whether the string matches any of the patterns in the blacklist
 */
export function matchesBlacklistPattern(text: string, blacklistPatterns: string[]): boolean {
  return blacklistPatterns.some(pattern => {
    const regexPattern = pattern.replace(/\*/g, ".*").replace(/\?/g, ".");
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(text);
  });
}
