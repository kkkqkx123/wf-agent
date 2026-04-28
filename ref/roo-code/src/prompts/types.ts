/**
 * Settings passed to system prompt generation functions
 */
export interface SystemPromptSettings {
	todoListEnabled: boolean
	useAgentRules: boolean
	/** When true, recursively discover and load .roo/rules from subdirectories */
	enableSubfolderRules?: boolean
	newTaskRequireTodos: boolean
	/** Whether skills functionality is globally enabled */
	skillsEnabled?: boolean
	/** List of disabled skill names */
	disabledSkills?: string[]
}
