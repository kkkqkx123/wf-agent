import { message } from '#lib/i18n';
import type { ExecutionKind } from '#lib/types/models';

/** Tab strip of the execution inspector, in display order.
 *
 * The inspector renders this list and the detail route restores `?tab=` from
 * it, so a tab added in one place is linkable from the other.
 */
export const EXECUTION_TABS: { id: ExecutionTab; label: () => string }[] = [
	{ id: 'overview', label: message('config.executionTabs.overview') },
	{ id: 'messages', label: message('config.executionTabs.messages') },
	{ id: 'variables', label: message('config.executionTabs.variables') },
	{ id: 'graph', label: message('config.executionTabs.graph') },
	{ id: 'trace', label: message('config.executionTabs.trace') },
	{ id: 'timeline', label: message('config.executionTabs.timeline') },
	{ id: 'tools', label: message('config.executionTabs.tools') },
	{ id: 'analysis', label: message('config.executionTabs.analysis') },
	{ id: 'state', label: message('config.executionTabs.state') },
	{ id: 'hierarchy', label: message('config.executionTabs.hierarchy') },
	{ id: 'history', label: message('config.executionTabs.history') },
	{ id: 'checkpoints', label: message('config.executionTabs.checkpoints') },
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
 * loop-specific messages/variables/checkpoints dimensions; chat is a
 * tool-free transcript, so it keeps the reading tabs without tools, graph,
 * analysis or checkpoints.
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
	agent: [
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
	chat: ['overview', 'messages', 'variables', 'timeline', 'hierarchy', 'history'],
};

/** Default tab shown when no (or an invalid) `?tab=` value is present. */
export function defaultTab(): ExecutionTab {
	return 'overview';
}

/** Whether a URL or caller-supplied value names a real execution tab. */
export function isExecutionTab(value: string | null): value is ExecutionTab {
	return value !== null && EXECUTION_TABS.some((tab) => tab.id === value);
}

/** Whether the value names a tab the given execution kind actually renders. */
export function isTabForKind(value: string | null, kind: ExecutionKind): value is ExecutionTab {
	return isExecutionTab(value) && TAB_GROUPS[kind].includes(value);
}
