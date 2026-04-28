export function getToolUseGuidelinesSection(): string {
	return `# Tool Use Guidelines

1. Assess what information you already have and what information you need to proceed with the task. If you lack basic information needed to proceed, use ask_followup_question to get clarification from the user.
2. Based on what you learn, choose the right tool for each subtask:
To read files, use read_file. Use slice mode for general exploration when you don't have a target line number. Use indentation mode with anchor_line when you have a specific line number from search results or errors, which ensures you get complete code blocks like whole functions rather than truncated snippets.
To modify existing files, use apply_patch. Never use write_to_file for editing existing files (except you want to completely rewrite).
To create new files, use write_to_file with complete content, or use edit_file with empty old_string.
To run system commands, use execute_command with relative paths when possible. Avoid commands that require interactive input. Always use 
3. When the task is complete, use attempt_completion to present the final result. If you need more information or hit an error you can't resolve, use ask_followup_question (you can also use it when you find the task is too large). Never assume tool outcomes without seeing the actual results.
`
}
