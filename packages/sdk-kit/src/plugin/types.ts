/**
 * SDK-Kit Plugin Types - Type definitions for the kit-level plugin API.
 */

// Imported from SDK to avoid duplication — @wf-agent/sdk is the single source of truth.
import type { ContributionType as SdkContributionType } from "@wf-agent/sdk/plugin";

export type ContributionType = SdkContributionType;

/**
 * Plugin information exposed by the kit-level PluginManager.
 */
export interface PluginInfo {
  id: string;
  version: string;
  name: string;
  status: string;
  contributions: ContributionType[];
  error?: string;
  activatedAt?: number;
}