/**
 * Formatting Rules Template
 * Used to define code formatting rules
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Template for formatting rules
 */
export const FORMAT_RULE_TEMPLATE: PromptTemplate = {
  id: "rules.format",
  name: "Format Rules",
  description: "Code formatting rules",
  category: "rules",
  content: `## Code Formatting Rules

### Basic Principles
1. **Consistency**: Maintain a consistent code style.
2. **Readability**: Prioritize the readability of the code.
3. **Simplicity**: Avoid unnecessary complexity.

### Indentation and Spacing
- Use 2 spaces for indentation.
- Add spaces before and after operators.
- Add a space after commas.
- Add a space after colons.

### Naming Conventions
- Use camelCase for variables and functions.
- Use PascalCase for classes and interfaces.
- Use UPPER_SNAKE_CASE for constants.
- Use an underscore prefix for private members.

### Commenting Conventions
- Add comments for complex logic.
- Comments should explain “why” rather than “what”.
- Keep comments up-to-date with the code.

### Line Length
- Each line of code should not exceed 80 characters.
- Long expressions should be broken into multiple lines.
- Maintain consistent line breaks.`,
  variables: [],
};
