/**
 * Environment Information Fragment Generator
 *
 * Provides runtime environment context, which is a static component (can be cached by the API).
 * Includes: operating system, time zone, user language, workspace path
 */

import type { EnvironmentInfo, WorkspaceInfo } from "@wf-agent/types";
import { wrapSection } from "./utils.js";

/**
 * Generate environment information content
 *
 * @param envInfo Environment information
 * @returns Formatted string of environment information
 */
export function generateEnvironmentContent(envInfo: EnvironmentInfo): string {
  const lines: string[] = [];

  // Workspace Information
  if (envInfo.workspaces && envInfo.workspaces.length > 0) {
    if (envInfo.workspaces.length === 1) {
      const firstWorkspace = envInfo.workspaces[0];
      if (firstWorkspace) {
        lines.push(`Current Workspace: ${firstWorkspace.path}`);
      }
    } else {
      lines.push("Multi-root Workspace:");
      for (const ws of envInfo.workspaces) {
        lines.push(`  - ${ws.name}: ${ws.path}`);
      }
      lines.push("");
      lines.push('Use "workspace_name/path" format to access files in specific workspace.');
    }
  } else {
    lines.push("No workspace open");
  }

  // Operating System
  if (envInfo.os) {
    lines.push(`Operating System: ${envInfo.os}`);
  }

  // Time zone
  if (envInfo.timezone) {
    lines.push(`Timezone: ${envInfo.timezone}`);
  }

  // User language
  if (envInfo.userLanguage) {
    lines.push(`User Language: ${envInfo.userLanguage}`);
    lines.push(`Please respond using the user's language by default.`);
  }

  return lines.join("\n");
}

/**
 * Generate environment information section
 */
export function generateEnvironmentSection(envInfo: EnvironmentInfo): string {
  const content = generateEnvironmentContent(envInfo);
  return wrapSection("ENVIRONMENT", content);
}

/**
 * Get default environment information
 *
 * @returns Default environment information
 */
export function getDefaultEnvironmentInfo(): EnvironmentInfo {
  // Obtain operating system information
  const os = getOperatingSystem();

  // Get the time zone
  const timezone = getTimezone();

  // Get the user's language
  const userLanguage = getUserLanguage();

  // The workspace information needs to be provided by the application layer.
  const workspaces: WorkspaceInfo[] = [];

  return {
    os,
    timezone,
    userLanguage,
    workspaces,
  };
}

/**
 * Obtain operating system information
 */
function getOperatingSystem(): string {
  const platform = process.platform;
  switch (platform) {
    case "win32":
      return "Windows";
    case "darwin":
      return "macOS";
    case "linux":
      return "Linux";
    default:
      return platform;
  }
}

/**
 * Get the time zone
 */
function getTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return "UTC";
  }
}

/**
 * Get the user's language
 */
function getUserLanguage(): string {
  // Use environment variables as a priority.
  const envLang = process.env["LANG"] || process.env["LC_ALL"] || process.env["LC_MESSAGES"];
  if (envLang) {
    // Extract the language code, e.g. "en_US.UTF-8" -> "en"
    const match = envLang.match(/^([a-z]{2})/i);
    if (match && match[1]) {
      return match[1].toLowerCase();
    }
  }

  // Default English
  return "en";
}

/**
 * Environment Information Fragment ID
 */
export const ENVIRONMENT_FRAGMENT_ID = "sdk.fragments.context.environment";
