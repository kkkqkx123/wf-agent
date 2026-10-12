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
	// global
	{ id: 'refresh', name: 'Refresh', aliases: ['r'], hint: 'r', hidden: false, weight: 100, danger: 'readonly', params: [], contexts: ['global'] },
	{ id: 'undo', name: 'Undo', aliases: ['Z'], hint: 'Z', hidden: true, weight: 8, danger: 'recoverable', params: [], contexts: ['global'] },
	{ id: 'focus', name: 'Focus filter', aliases: ['/'], hint: '/', hidden: true, weight: 10, danger: 'readonly', params: ['path'], contexts: ['global'] },
	{ id: 'filter', name: 'Filter', aliases: ['f'], hint: 'f', hidden: true, weight: 9, danger: 'readonly', params: ['query'], contexts: ['global'] },
	// files
	{ id: 'stage.toggle', name: 'Toggle stage', aliases: [' '], hint: 'space', hidden: false, weight: 95, danger: 'recoverable', params: ['path'], contexts: ['files'] },
	{ id: 'stage.all', name: 'Stage all', aliases: ['a'], hint: 'a', hidden: false, weight: 80, danger: 'recoverable', params: [], contexts: ['files'] },
	{ id: 'commit', name: 'Commit', aliases: ['c'], hint: 'c', hidden: false, weight: 90, danger: 'recoverable', params: ['message'], contexts: ['files'] },
	{ id: 'commit.direct', name: 'Commit direct', aliases: ['C'], hint: 'C', hidden: true, weight: 65, danger: 'recoverable', params: ['message'], contexts: ['files'] },
	{ id: 'commit.amend', name: 'Amend', aliases: ['A'], hint: 'A', hidden: true, weight: 30, danger: 'history_rewrite', params: [], contexts: ['files'] },
	{ id: 'discard', name: 'Discard', aliases: ['x'], hint: 'x', hidden: false, weight: 70, danger: 'destructive', params: ['path'], contexts: ['files'] },
	{ id: 'stash.push', name: 'Stash', aliases: ['s'], hint: 's', hidden: false, weight: 75, danger: 'recoverable', params: ['message'], contexts: ['files'] },
	{ id: 'unstage.all', name: 'Unstage all', aliases: ['u'], hint: 'u', hidden: true, weight: 40, danger: 'recoverable', params: [], contexts: ['files'] },
	{ id: 'ignore', name: 'Ignore', aliases: ['i'], hint: 'i', hidden: true, weight: 5, danger: 'recoverable', params: ['path'], contexts: ['files'] },
	// commits
	{ id: 'commit.revert', name: 'Revert', aliases: ['v'], hint: 'v', hidden: false, weight: 85, danger: 'recoverable', params: ['commit'], contexts: ['commits'] },
	{ id: 'commit.cherry-pick', name: 'Cherry-pick', aliases: ['y'], hint: 'y', hidden: true, weight: 60, danger: 'recoverable', params: ['commit'], contexts: ['commits'] },
	{ id: 'commit.to-branch', name: 'Branch at commit', aliases: ['b'], hint: 'b', hidden: true, weight: 55, danger: 'recoverable', params: ['commit', 'branch'], contexts: ['commits'] },
	{ id: 'commit.to-tag', name: 'Tag at commit', aliases: ['t'], hint: 't', hidden: true, weight: 50, danger: 'recoverable', params: ['commit', 'tag'], contexts: ['commits'] },
	// branches
	{ id: 'branch.checkout', name: 'Checkout', aliases: ['o'], hint: 'o', hidden: false, weight: 85, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.create', name: 'New branch', aliases: ['n'], hint: 'n', hidden: true, weight: 60, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.delete', name: 'Delete branch', aliases: ['d'], hint: 'd', hidden: false, weight: 65, danger: 'destructive', params: ['branch'], contexts: ['branches'] },
	{ id: 'push', name: 'Push', aliases: ['p'], hint: 'p', hidden: false, weight: 55, danger: 'history_rewrite', params: ['remote', 'branch'], contexts: ['branches'] },
	{ id: 'pull', name: 'Pull', aliases: ['l'], hint: 'l', hidden: false, weight: 50, danger: 'recoverable', params: ['remote', 'branch'], contexts: ['branches'] },
	{ id: 'branch.merge', name: 'Merge', aliases: ['m'], hint: 'm', hidden: false, weight: 45, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.rename', name: 'Rename', aliases: ['R'], hint: 'R', hidden: true, weight: 35, danger: 'recoverable', params: ['from', 'to'], contexts: ['branches'] },
	{ id: 'branch.fast-forward', name: 'Fast-forward', aliases: ['F'], hint: 'F', hidden: true, weight: 30, danger: 'recoverable', params: ['branch'], contexts: ['branches'] },
	{ id: 'branch.rebase', name: 'Rebase', aliases: ['e'], hint: 'e', hidden: true, weight: 25, danger: 'history_rewrite', params: ['onto'], contexts: ['branches'] },
	// stash
	{ id: 'stash.pop', name: 'Pop', aliases: ['p'], hint: 'p', hidden: false, weight: 90, danger: 'recoverable', params: ['index'], contexts: ['stash'] },
	{ id: 'stash.apply', name: 'Apply', aliases: ['a'], hint: 'a', hidden: false, weight: 80, danger: 'recoverable', params: ['index'], contexts: ['stash'] },
	{ id: 'stash.drop', name: 'Drop', aliases: ['d'], hint: 'd', hidden: false, weight: 65, danger: 'destructive', params: ['index'], contexts: ['stash'] },
	{ id: 'stash.to-branch', name: 'Branch from stash', aliases: ['b'], hint: 'b', hidden: true, weight: 40, danger: 'recoverable', params: ['branch', 'index'], contexts: ['stash'] },
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
