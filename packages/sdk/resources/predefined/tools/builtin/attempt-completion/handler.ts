import type { ToolOutput, BuiltinToolExecutionContext } from "@wf-agent/types";

export function createAttemptCompletionHandler() {
  return async (
    params: Record<string, unknown>,
    _context: BuiltinToolExecutionContext,
  ): Promise<ToolOutput> => {
    try {
      const { data, variables } = params as {
        data?: Record<string, unknown>;
        variables?: Record<string, unknown>;
      };

      return {
        success: true,
        content: "Task completed successfully.",
        metadata: {
          type: "completion",
          data: data ?? null,
          variables: variables ?? null,
        },
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
