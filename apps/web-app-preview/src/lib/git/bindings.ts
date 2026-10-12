export type DangerLevel = 'readonly' | 'recoverable' | 'destructive' | 'history_rewrite';

export interface BindingItem {
	id: string;
	name: string;
	aliases: string[];
	hint: string;
	hidden: boolean;
	weight: number;
	danger: DangerLevel;
	params: string[];
	contexts: string[];
}

const BINDINGS: BindingItem[] = [
	{ id: 'refresh', name: 'Refresh', aliases: ['r'], hint: 'r', hidden: false, weight: 100, danger: 'readonly', params: [], contexts: ['global'] },
	{ id: 'stage.toggle', name: 'Toggle stage', aliases: [' '], hint: 'space', hidden: false, weight: 95, danger: 'recoverable', params: ['path'], contexts: ['files'] },
	{ id: 'stage.all', name: 'Stage all', aliases: ['a'], hint: 'a', hidden: false, weight: 80, danger: 'recoverable', params: [], contexts: ['files'] },
	{ id: 'commit', name: 'Commit', aliases: ['c'], hint: 'c', hidden: false, weight: 90, danger: 'recoverable', params: ['message'], contexts: ['files'] },
	{ id: 'discard', name: 'Discard', aliases: ['x'], hint: 'x', hidden: false, weight: 70, danger: 'destructive', params: ['path'], contexts: ['files'] },
	{ id: 'stash.push', name: 'Stash', aliases: ['s'], hint: 's', hidden: false, weight: 75, danger: 'recoverable', params: ['message'], contexts: ['files'] },
	{ id: 'branch.checkout', name: 'Checkout', aliases: ['o'], hint: 'o', hidden: false, weight: 85, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.create', name: 'New branch', aliases: ['n'], hint: 'n', hidden: true, weight: 60, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.delete', name: 'Delete branch', aliases: ['d'], hint: 'd', hidden: false, weight: 65, danger: 'destructive', params: ['branch'], contexts: ['branches'] },
	{ id: 'push', name: 'Push', aliases: ['p'], hint: 'p', hidden: false, weight: 55, danger: 'history_rewrite', params: ['remote', 'branch'], contexts: ['branches'] },
	{ id: 'pull', name: 'Pull', aliases: ['l'], hint: 'l', hidden: false, weight: 50, danger: 'recoverable', params: ['remote', 'branch'], contexts: ['branches'] },
	{ id: 'focus', name: 'Focus filter', aliases: ['/'], hint: '/', hidden: true, weight: 10, danger: 'readonly', params: ['query'], contexts: ['global'] },
];

export const GIT_BINDINGS: BindingItem[] = BINDINGS;

export function bindingsFor(context: string): BindingItem[] {
	return BINDINGS.filter((b) => b.contexts.includes(context) || b.contexts.includes('global')).sort(
		(a, b) => b.weight - a.weight,
	);
}

export function visibleControls(context: string, limit = 6): BindingItem[] {
	return bindingsFor(context)
		.filter((b) => !b.hidden)
		.slice(0, limit);
}

export function findBinding(id: string): BindingItem | undefined {
	return BINDINGS.find((b) => b.id === id);
}

export function checkDrift(registryIds: string[]): { unknown: string[]; missing: string[] } {
	const local = new Set(BINDINGS.map((b) => b.id));
	const remote = new Set(registryIds);
	return {
		unknown: [...remote].filter((id) => !local.has(id)),
		missing: [...local].filter((id) => !remote.has(id)),
	};
}
