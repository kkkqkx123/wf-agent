/**
 * Code Review Instruction Template
 * Used to define user instructions for code reviews
 */

import type { PromptTemplate } from "@wf-agent/prompt-templates";

/**
 * Code Review Instruction Template
 */
export const CODE_REVIEW_TEMPLATE: PromptTemplate = {
  id: "user_commands.code_review",
  name: "Code Review Command",
  description: "Code review instructions",
  category: "user-command",
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
