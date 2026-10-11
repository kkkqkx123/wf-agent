import type { ExecutionKind } from '$lib/types/models';

/** Tab strip of the execution inspector, in display order.
 *
 * The inspector renders this list and the detail route restores `?tab=` from
 * it, so a tab added in one place is linkable from the other.
 */
export const EXECUTION_TABS: { id: ExecutionTab; label: string }[] = [
	{ id: 'overview', label: 'Overview' },
	{ id: 'messages', label: 'Messages' },
	{ id: 'variables', label: 'Variables' },
	{ id: 'graph', label: 'Graph' },
	{ id: 'trace', label: 'Trace' },
	{ id: 'timeline', label: 'Timeline' },
	{ id: 'tools', label: 'Tools' },
	{ id: 'analysis', label: 'Analysis' },
	{ id: 'state', label: 'State' },
	{ id: 'hierarchy', label: 'Hierarchy' },
	{ id: 'history', label: 'History' },
	{ id: 'checkpoints', label: 'Checkpoints' },
];

export type ExecutionTab =
	| 'overview'
	| 'messages'
	| 'variables'
	| 'graph'
	| 'trace'
	| 'timeline'
	| 'tools'
	| 'analysis'
	| 'state'
	| 'hierarchy'
	| 'history'
	| 'checkpoints';

/** Tabs each execution kind offers, in display order.
 *
 * Workflow keeps the nine inspector tabs; agent loops swap trace/state for the
 * loop-specific messages/variables/checkpoints dimensions.
 */
export const TAB_GROUPS: Record<ExecutionKind, ExecutionTab[]> = {
	workflow: [
		'overview',
		'graph',
		'trace',
		'timeline',
		'tools',
		'analysis',
		'state',
		'hierarchy',
		'history',
	],
	agent_loop: [
		'overview',
		'messages',
		'variables',
		'graph',
		'tools',
		'timeline',
		'analysis',
		'checkpoints',
		'hierarchy',
		'history',
	],
};

/** Default tab shown when no (or an invalid) `?tab=` value is present. */
export function defaultTab(kind: ExecutionKind): ExecutionTab {
	return kind === 'agent_loop' ? 'overview' : 'overview';
}

/** Whether a URL or caller-supplied value names a real execution tab. */
export function isExecutionTab(value: string | null): value is ExecutionTab {
	return value !== null && EXECUTION_TABS.some((tab) => tab.id === value);
}

/** Whether the value names a tab the given execution kind actually renders. */
export function isTabForKind(
	value: string | null,
	kind: ExecutionKind,
): value is ExecutionTab {
	return isExecutionTab(value) && TAB_GROUPS[kind].includes(value);
}
