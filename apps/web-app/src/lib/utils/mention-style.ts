import type { IconName } from '#lib/ui/icons/paths';

export type MentionTone =
	| 'file'
	| 'directory'
	| 'skill'
	| 'workflow'
	| 'agent'
	| 'tool'
	| 'script'
	| 'resource'
	| 'neutral';

/** Map a mention kind to a stable tone used by style helpers. */
export function mentionTone(kind: string, label?: string): MentionTone {
	if (kind === 'file' && label?.trim().endsWith('/')) return 'directory';
	switch (kind) {
		case 'file':
			return 'file';
		case 'skill':
			return 'skill';
		case 'workflow':
		case 'wf':
			return 'workflow';
		case 'agent':
		case 'agent_loop':
			return 'agent';
		case 'tool':
			return 'tool';
		case 'script':
			return 'script';
		case 'resource':
			return 'resource';
		default:
			return 'neutral';
	}
}

/** Icon name for a mention kind, using the shared icon set. */
export function mentionIcon(kind: string, label?: string): IconName {
	if (kind === 'file' && label?.trim().endsWith('/')) {
		return 'folder';
	}
	switch (mentionTone(kind)) {
		case 'file':
			return 'file';
		case 'skill':
			return 'sparkles';
		case 'workflow':
			return 'workflow';
		case 'agent':
			return 'user';
		case 'tool':
			return 'terminal';
		case 'script':
			return 'code';
		case 'resource':
			return 'link';
		default:
			return 'hash';
	}
}

/** Tailwind classes for a mention token, keeping layout in the base TAG. */
export function mentionClass(kind: string, label?: string): string {
	switch (mentionTone(kind, label)) {
		case 'file':
			return 'text-info border-info/40';
		case 'directory':
			return 'text-success border-success/40';
		case 'skill':
			return 'text-primary border-primary/40';
		case 'workflow':
			return 'text-info border-info/40';
		case 'agent':
			return 'text-success border-success/40';
		case 'tool':
			return 'text-warning border-warning/40';
		case 'script':
			return 'text-warning border-warning/40';
		case 'resource':
			return 'text-accent border-accent/40';
		default:
			return 'text-muted-foreground border-border/70';
	}
}
