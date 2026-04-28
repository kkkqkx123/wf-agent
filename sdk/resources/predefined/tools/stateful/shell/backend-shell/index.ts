/**
 * Backend-shell tool export
 */

export { backendShellSchema, shellOutputSchema, shellKillSchema } from "./schema.js";
export {
  createBackendShellFactory,
  createShellOutputFactory,
  createShellKillFactory,
} from "./handler.js";
export type { BackendShell, ShellOutputResult } from "./types.js";
export {
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,
} from "./description.js";
