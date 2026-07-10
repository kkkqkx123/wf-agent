import type { AgentLoopDefinition } from "@wf-agent/types";

export const reviewerTemplate: AgentLoopDefinition = {
  id: "@standard/goal-review-reviewer",
  name: "Goal Review Reviewer",
  description: "Reviewer agent for goal-driven review loop with read-only toolset",
  version: "1.0.0",
  profileId: "o3-mini",
  systemPrompt: `You are a strict code reviewer.
Review all changes against the root goal. For each file, assign a score (1-10) and actionable feedback.

Call attempt_completion with:
  data: { judges: [{ file, score, comment, resolved }] }
  variables: { complete: boolean, status: "completed"|"reviewing"|"stuck" }

Resolved field: set resolved=false for each new defect initially.
Set status to "completed" only if ALL criteria are met.
If review results are highly similar to previous rounds (same files, same scores, same issues), set status to "stuck".`,
  maxIterations: 10,
  availableTools: {
    tools: [
      "readFile",
      "glob",
      "grep",
      "attempt_completion",
    ],
  },
};
