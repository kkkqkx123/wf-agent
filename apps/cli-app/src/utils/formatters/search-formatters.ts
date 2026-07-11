/**
 * Search Formatters
 * Format search results for CLI output
 */

import type { SearchResultItem, SearchResourceType } from "@wf-agent/sdk/api";

export type SearchResults = Record<SearchResourceType, SearchResultItem[]>;

const typeEmojis: Partial<Record<SearchResourceType, string>> = {
  workflow: "⚙️",
  execution: "▶️",
  task: "📋",
  checkpoint: "💾",
  event: "📡",
  agent_loop: "🤖",
};

export function formatSearchResults(results: SearchResults): string {
  const lines: string[] = [];

  if (!results || Object.keys(results).length === 0) {
    lines.push("");
    lines.push("No results found");
    lines.push("");
    return lines.join("\n");
  }

  lines.push("");
  lines.push("🔍 Search Results");
  lines.push("");

  let totalCount = 0;
  for (const [type, items] of Object.entries(results)) {
    if (!items || items.length === 0) continue;

    const emoji = typeEmojis[type as SearchResourceType] || "📌";
    lines.push(`${emoji} ${type.toUpperCase()} (${items.length})`);

    for (const item of items) {
      const relevance = item.score ? ` [${item.score.toFixed(2)}]` : "";
      lines.push(`  • ${item.label}${relevance}`);
      lines.push(`    ID: ${item.id}`);
    }
    lines.push("");
    totalCount += items.length;
  }

  lines.push(`Total: ${totalCount} results found`);
  lines.push("");

  return lines.join("\n");
}

export function formatSearchResultsTable(results: SearchResults): string[] {
  const lines: string[] = [];

  for (const [type, items] of Object.entries(results)) {
    if (!items || items.length === 0) continue;

    const emoji = typeEmojis[type as SearchResourceType] || "📌";
    lines.push(`\n${emoji} ${type.toUpperCase()}`);
    lines.push("─".repeat(60));

    for (const item of items) {
      const relevance = item.score ? `[${item.score.toFixed(2)}]` : "";
      lines.push(`${item.label.padEnd(40)} ${relevance}`);
      lines.push(`  → ${item.id}`);
    }
  }

  return lines;
}
