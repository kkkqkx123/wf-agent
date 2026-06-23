/**
 * Code Review Task Instruction Fragment
 * Injected as a system prompt fragment for code review tasks.
 */

import type { SystemPromptFragment } from "@wf-agent/types";

export const CODE_REVIEW_FRAGMENT: SystemPromptFragment = {
  id: "fragments.task-instruction.code-review",
  category: "task-instruction",
  description: "Code review task instructions",
  content: `Please review the following code:

## Review points:
1. **Code quality**: Check the readability and maintainability of the code.
2. **Functionality correctness**: Verify that the code logic is correct.
3. **Performance issues**: Identify potential performance bottlenecks.
4. **Security**: Check for any security vulnerabilities.
5. **Best practices**: Assess whether the code follows coding standards and best practices.

## Review content:
{{codeContent}}

## Review requirements:
- Provide specific suggestions for improvement.
- Highlight potential issues and risks.
- Assign priorities.
- Provide examples of how to fix the issues (if applicable).
- Stay constructive and objective.`,
  variables: [
    {
      name: "codeContent",
      type: "string",
      required: true,
      description: "The code content to be reviewed",
    },
  ],
};
