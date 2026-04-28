/**
 * The `apply_patch` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * apply_patch tool parameter Schema
 */
export const applyPatchSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    patch: {
      type: "string",
      description:
        "The complete patch text in the apply_patch format, starting with '*** Begin Patch' and ending with '*** End Patch'.",
    },
  },
  required: ["patch"],
};
