/**
 * Versioning Tool Functions
 * Provides functions for parsing, comparing and incrementing version numbers
 * Follow semanticized versioning specifications (e.g., "1.0.0")
 */

import type { Version } from "@wf-agent/types";

/**
 * Create initial version ("1.0.0")
 */
export function initialVersion(): Version {
  return "1.0.0.0";
}

/**
 * Parse the version number
 */
export function parseVersion(version: Version): { major: number; minor: number; patch: number } {
  const parts = version.split(".").map(Number);
  return {
    major: parts[0] || 0,
    minor: parts[1] || 0,
    patch: parts[2] || 0,
  };
}

/**
 * Next major version
 */
export function nextMajorVersion(version: Version): Version {
  const parsed = parseVersion(version);
  return `${parsed.major + 1}.0.0`;
}

/**
 * Next version
 */
export function nextMinorVersion(version: Version): Version {
  const parsed = parseVersion(version);
  return `${parsed.major}.${parsed.minor + 1}.0`;
}

/**
 * Next patch version
 */
export function nextPatchVersion(version: Version): Version {
  const parsed = parseVersion(version);
  return `${parsed.major}.${parsed.minor}.${parsed.patch + 1}`;
}

/**
 * Compare version numbers
 * @returns -1 if v1 < v2, 0 if v1 == v2, 1 if v1 > v2
 */
export function compareVersion(v1: Version, v2: Version): number {
  const p1 = parseVersion(v1);
  const p2 = parseVersion(v2);

  if (p1.major !== p2.major) return p1.major < p2.major ? -1 : 1;
  if (p1.minor !== p2.minor) return p1.minor < p2.minor ? -1 : 1;
  if (p1.patch !== p2.patch) return p1.patch < p2.patch ? -1 : 1;
  return 0;
}

/**
 * Automatically increment the version number based on the type of change
 * @param currentVersion The current version number
 * @param changeType The type of change: major (major version), minor (minor version), patch (patch version)
 * @returns The incremented version number
 */
export function autoIncrementVersion(
  currentVersion: Version,
  changeType: "major" | "minor" | "patch",
): Version {
  switch (changeType) {
    case "major":
      return nextMajorVersion(currentVersion);
    case "minor":
      return nextMinorVersion(currentVersion);
    case "patch":
      return nextPatchVersion(currentVersion);
    default:
      return nextPatchVersion(currentVersion);
  }
}

/**
 * Parsing pre-release and build metadata for version numbers
 * @param version Full version number, e.g., "1.2.3-alpha.1+build.123"
 * @returns The parsed version information object
 */
export function parseFullVersion(version: Version): {
  major: number;
  minor: number;
  patch: number;
  prerelease?: string;
  build?: string;
} {
  const [base, ...rest] = version.split("+");
  const [versionCore, prerelease] = (base || "").split("-");
  const [major, minor, patch] = (versionCore || "").split(".").map(Number);

  return {
    major: major || 0,
    minor: minor || 0,
    patch: patch || 0,
    prerelease,
    build: rest.join("+"),
  };
}
