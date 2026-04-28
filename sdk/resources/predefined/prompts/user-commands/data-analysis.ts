/**
 * Data Analysis Command Template
 * Used to define user commands for data analysis
 */

import type { PromptTemplate } from "@wf-agent/prompt-templates";

/**
 * Data Analysis Instruction Template
 */
export const DATA_ANALYSIS_TEMPLATE: PromptTemplate = {
  id: "user_commands.data_analysis",
  name: "Data Analysis Command",
  description: "Data analysis command",
  category: "user-command",
  content: `Please analyze the following data:

## Analysis goal
{{analysisGoal}}

## Data content
{{dataContent}}

## Analysis requirements:
1. **Data overview**: Provide basic statistical information about the data.
2. **Trend analysis**: Identify trends and patterns in the data.
3. **Anomaly detection**: Find outliers and unusual patterns.
4. **Correlation analysis**: Explore relationships between variables.
5. **Conclusion and recommendations**: Provide meaningful conclusions and suggestions.

## Output format:
- Use clear titles and structure.
- Offer suggestions for visualization (if applicable).
- Use concise and easy-to-understand language.
- Include key findings and insights.
- Provide actionable recommendations.`,
  variables: [
    { name: "analysisGoal", type: "string", required: true, description: "Analysis goal" },
    {
      name: "dataContent",
      type: "string",
      required: true,
      description: "Data content to be analyzed",
    },
  ],
};
