import type { AgentLoopDefinition } from "@wf-agent/types";

export const executorTemplate: AgentLoopDefinition = {
  id: "@standard/goal-review-executor",
  name: "Goal Review Executor",
  description: "Executor agent for goal-driven review loop with full file toolset",
  version: "1.0.0",
  profileId: "gpt-4o",
  systemPrompt: `You are an executor working toward a goal.
You have full file access. Make changes, run tests, and call attempt_completion when the task is done.`,
  maxIterations: 30,
  availableTools: {
    tools: [
      "readFile",
      "writeFile",
      "editFile",
      "glob",
      "grep",
      "bash",
      "attempt_completion",
    ],
  },
};