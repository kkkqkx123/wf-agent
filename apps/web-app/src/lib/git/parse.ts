import { GIT_BINDINGS } from './bindings';

export interface TokenResult {
	kind: 'empty' | 'action' | 'command' | 'help' | 'prefix';
	action?: string;
	danger?: string;
	preview?: string;
	candidates?: string[];
}

/** Actions that only touch client view state; they never reach the server. */
export const CLIENT_ONLY_ACTIONS: ReadonlySet<string> = new Set(['focus', 'filter']);

/**
 * Confirmation token the server expects for guarded actions: the client must
 * echo the exact target the backend will compare against.
 */
export function confirmTarget(action: string, params: Record<string, string>): string {
	const value = (key: string): string => (params[key] ?? '').trim();
	switch (action) {
		case 'push':
			return [value('remote') || 'origin', value('branch')].filter(Boolean).join(' ');
		case 'stash.drop':
			return `stash@{${value('index') || '0'}}`;
		case 'commit.amend':
			return 'amend';
		default:
			return value('branch') || value('path') || value('commit') || value('base') || value('onto');
	}
}

function scopedBindings(context: string) {
	return GIT_BINDINGS.filter(
		(b) => b.contexts.includes(context) || b.contexts.includes('global'),
	);
}

export function parseToken(input: string, context: string): TokenResult {
	if (input === ' ') {
		const hit = scopedBindings(context).find((b) => b.aliases.includes(' '));
		if (hit) {
			return {
				kind: 'action',
				action: hit.id,
				danger: hit.danger,
				preview: `${hit.name} (${hit.id})`,
			};
		}
	}
	const text = input.trim();
	if (!text) {
		return {
			kind: 'empty',
			preview: `Type a key in ${context}; ? lists actions`,
		};
	}
	if (text === '?') {
		return {
			kind: 'help',
			candidates: GIT_BINDINGS.filter(
				(b) => b.contexts.includes(context) || b.contexts.includes('global'),
			).map((b) => b.id),
		};
	}
	if (text.startsWith(':')) {
		return { kind: 'command', preview: text.slice(1) };
	}
	if (text.length === 1) {
		const hit = scopedBindings(context).find((b) => b.aliases.includes(text));
		if (hit) {
			return {
				kind: 'action',
				action: hit.id,
				danger: hit.danger,
				preview: `${hit.name} (${hit.id})`,
			};
		}
		const candidates = GIT_BINDINGS.filter((b) => b.id.startsWith(text)).map((b) => b.id);
		return { kind: 'prefix', candidates };
	}
	const aliasHit = scopedBindings(context).find((b) => b.aliases.includes(text));
	if (aliasHit) {
		return {
			kind: 'action',
			action: aliasHit.id,
			danger: aliasHit.danger,
			preview: `${aliasHit.name} (${aliasHit.id})`,
		};
	}
	const lowered = text.toLowerCase();
	const scored = GIT_BINDINGS.filter(
		(b) =>
			b.id.startsWith(text) ||
			b.id.endsWith(`.${lowered}`) ||
			b.id.toLowerCase().includes(lowered) ||
			b.hint.toLowerCase() === lowered ||
			(b.legacyKey ?? '').toLowerCase() === lowered,
	).sort((a, b) => {
		const aLocal = a.contexts.includes(context) ? 0 : 1;
		const bLocal = b.contexts.includes(context) ? 0 : 1;
		return aLocal - bLocal || b.weight - a.weight;
	});
	if (scored.length === 1) {
		return {
			kind: 'action',
			action: scored[0].id,
			danger: scored[0].danger,
			preview: `${scored[0].name} (${scored[0].id})`,
		};
	}
	return { kind: 'prefix', candidates: scored.map((b) => b.id) };
}
