/**
 * Agent Loop & Skill formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, truncate, shortId, emptyMsg } from "./utils.js";
import type { AgentLoopResult, Skill } from "@wf-agent/types";

// ============================================
// Agent Loop Formatters
// ============================================

// Type alias for agent loop with additional fields
type AgentLoopWithMetadata = AgentLoopResult & {
  id?: string;
  status?: string;
  currentIteration?: number;
  content?: string;
};

export function formatAgentLoop(agentLoop: AgentLoopWithMetadata, options?: { verbose?: boolean }): string {
  return formatWith(agentLoop, options, () => {
    const id = agentLoop.id || "N/A";
    const status = agentLoop.status || agentLoop.success ? "completed" : "failed";
    const iterations = agentLoop.iterations ?? agentLoop.currentIteration ?? 0;
    const toolCallCount = agentLoop.toolCallCount ?? 0;

    let content = "";
    if (agentLoop.content) {
      content = `\n  Result: ${truncate(agentLoop.content, 50)}`;
    }

    return `${id} - ${status} - Iterations: ${iterations} - Tool calls: ${toolCallCount}${content}`;
  });
}

export function formatAgentLoopList(agentLoops: AgentLoopWithMetadata[], options?: { table?: boolean }): string {
  if (agentLoops.length === 0) {
    return emptyMsg("agent loops");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["ID", "Status", "Number of iterations", "Tool invocation"];
    const rows = agentLoops.map(al => [
      shortId(al.id),
      al.status || "unknown",
      String(al.iterations ?? al.currentIteration ?? 0),
      String(al.toolCallCount ?? 0),
    ]);
    return formatter.table(headers, rows);
  }

  return agentLoops.map(al => formatAgentLoop(al)).join("\n");
}

// ============================================
// Skill Formatters
// ============================================

// Type alias for skill with version and description fields
type SkillWithMetadata = Skill & {
  name?: string;
  version?: string;
  description?: string;
};

export function formatSkill(skill: SkillWithMetadata, options?: { verbose?: boolean }): string {
  return formatWith(skill, options, () => {
    const name = skill.name || "N/A";
    const version = skill.version || "N/A";
    const description = skill.description || "-";
    return `${name} (${version}) - ${description}`;
  });
}

export function formatSkillList(skills: SkillWithMetadata[], options?: { table?: boolean }): string {
  if (skills.length === 0) {
    return emptyMsg("skills");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Name", "Version", "Description"];
    const rows = skills.map(s => [s.name || "N/A", s.version || "N/A", s.description || "-"]);
    return formatter.table(headers, rows);
  }

  return skills.map(s => formatSkill(s)).join("\n");
}