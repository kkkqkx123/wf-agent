/** Tab strip of the execution inspector, in display order.
 *
 * The inspector renders this list and the detail route restores `?tab=` from
 * it, so a tab added in one place is linkable from the other.
 */
export const EXECUTION_TABS: { id: ExecutionTab; label: string }[] = [
	{ id: 'overview', label: 'Overview' },
	{ id: 'graph', label: 'Graph' },
	{ id: 'trace', label: 'Trace' },
	{ id: 'timeline', label: 'Timeline' },
	{ id: 'tools', label: 'Tools' },
	{ id: 'analysis', label: 'Analysis' },
	{ id: 'state', label: 'State' },
	{ id: 'hierarchy', label: 'Hierarchy' },
	{ id: 'history', label: 'History' },
];

export type ExecutionTab =
	| 'overview'
	| 'graph'
	| 'trace'
	| 'timeline'
	| 'tools'
	| 'analysis'
	| 'state'
	| 'hierarchy'
	| 'history';

/** Whether a URL or caller-supplied value names a real execution tab. */
export function isExecutionTab(value: string | null): value is ExecutionTab {
	return value !== null && EXECUTION_TABS.some((tab) => tab.id === value);
}
