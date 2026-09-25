export type CommandAction = 'new' | 'retry' | 'continue';

export interface SlashCommand {
	name: string;
	/** Argument the command accepts; null when it takes none. */
	arg: string | null;
	description: string;
	action: CommandAction;
}

/** The whole command vocabulary: a new command is a new row here. */
export const SLASH_COMMANDS: SlashCommand[] = [
	{
		name: 'new',
		arg: null,
		description: 'Start a fresh draft session',
		action: 'new',
	},
	{
		name: 'retry',
		arg: null,
		description: 'Resend the last user message',
		action: 'retry',
	},
	{
		name: 'continue',
		arg: '[note]',
		description: 'Keep the run going, optionally steering it',
		action: 'continue',
	},
];

export interface ParsedCommand {
	command: SlashCommand;
	arg: string;
}

/** Only a known command name parses; anything else is an ordinary message. */
export function parseCommand(text: string): ParsedCommand | null {
	if (!text.startsWith('/')) return null;
	const [token = '', ...rest] = text.slice(1).split(/\s+/);
	const command = SLASH_COMMANDS.find((entry) => entry.name === token);
	if (!command) return null;
	return { command, arg: rest.join(' ').trim() };
}

export function commandLabel(command: SlashCommand): string {
	return command.arg ? `/${command.name} ${command.arg}` : `/${command.name}`;
}

/** Commands whose name still fits the typed prefix. */
export function matchCommands(prefix: string): SlashCommand[] {
	return SLASH_COMMANDS.filter((entry) => entry.name.startsWith(prefix));
}
