/**
 * LLM Profile formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { LLMProfile } from "@wf-agent/types";

export function formatLLMProfile(profile: LLMProfile, options?: { verbose?: boolean }): string {
  return formatWith(profile, options, () => {
    const provider = profile.provider || "N/A";
    const model = profile.model || "N/A";
    const baseUrl = profile.baseUrl || "default";
    return `${profile.name || "unnamed"} (${profile.id || "N/A"}) - ${provider} - ${model} - ${baseUrl}`;
  });
}

export function formatLLMProfileList(profiles: LLMProfile[], options?: { table?: boolean }): string {
  if (profiles.length === 0) {
    return emptyMsg("LLM profiles");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["ID", "Name", "Provider", "Model", "Basic URL"];
    const rows = profiles.map(p => [
      shortId(p.id),
      p.name || "unnamed",
      p.provider || "N/A",
      p.model || "N/A",
      p.baseUrl || "default",
    ]);
    return formatter.table(headers, rows);
  }

  return profiles.map(p => formatLLMProfile(p)).join("\n");
}