/**
 * Backend-shell Tool Description Definition
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const BACKEND_SHELL_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "backend_shell",
  id: "backend_shell",
  type: "STATEFUL",
  category: "shell",
  description:
    "Execute shell commands in background for long-running processes. Use this for starting servers, long-running build processes, or background tasks that need monitoring. After starting, use shell_output to monitor and shell_kill to terminate.",
  parameters: [
    {
      name: "command",
      type: "string",
      required: true,
      description: "The shell command to execute in background",
    },
  ],
  tips: [
    "Use for starting servers (npm run dev, python -m http.server)",
    "Use for long-running build processes",
    "After starting, use shell_output to monitor",
    "Use shell_kill to terminate when done",
  ],
};

export const SHELL_OUTPUT_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "shell_output",
  id: "shell_output",
  type: "STATEFUL",
  category: "shell",
  description:
    "Retrieves output from a running or completed background shell. Takes a shell_id parameter identifying the shell. Always returns only new output since the last check. Returns stdout and stderr output along with shell status.",
  parameters: [
    {
      name: "shell_id",
      type: "string",
      required: true,
      description: "The ID of the background shell to retrieve output from",
    },
    {
      name: "filter_str",
      type: "string",
      required: false,
      description: "Optional regular expression to filter the output lines",
    },
  ],
  tips: [
    "Always returns only new output since the last check",
    "Use filter_str to show only lines matching a pattern",
    "Check status to know if process is still running",
  ],
};

export const SHELL_KILL_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "shell_kill",
  id: "shell_kill",
  type: "STATEFUL",
  category: "shell",
  description:
    "Kills a running background shell by its ID. Attempts graceful termination (SIGTERM) first, then forces (SIGKILL) if needed. Returns the final status and any remaining output before termination.",
  parameters: [
    {
      name: "shell_id",
      type: "string",
      required: true,
      description: "The ID of the background shell to terminate",
    },
  ],
  tips: [
    "Attempts graceful termination first, then force kill if needed",
    "Returns final status and any remaining output",
    "Cleans up all resources associated with the shell",
  ],
};
