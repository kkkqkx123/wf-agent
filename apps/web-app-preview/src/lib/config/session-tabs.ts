/**
 * Tabs a single agent loop session can be inspected through. The transcript is
 * separate because only the full detail page shows it as a tab; the chat column
 * already renders it.
 */
export type SessionTab =
	| 'overview'
	| 'graph'
	| 'iterations'
	| 'variables'
	| 'checkpoints'
	| 'tools'
	| 'timeline'
	| 'analysis'
	| 'messages';

export const TAB_LABELS: Record<SessionTab, string> = {
	overview: 'Overview',
	graph: 'Graph',
	iterations: 'Iterations',
	variables: 'Variables',
	checkpoints: 'Checkpoints',
	tools: 'Tools',
	timeline: 'Timeline',
	analysis: 'Analysis',
	messages: 'Messages',
};

/** The eight tabs the session inspector offers. */
export const INSPECTOR_TABS: SessionTab[] = [
	'overview',
	'graph',
	'iterations',
	'variables',
	'checkpoints',
	'tools',
	'timeline',
	'analysis',
];

/** Detail-page tab set: the inspector tabs plus the stored transcript. */
export const DETAIL_TABS: SessionTab[] = ['messages', ...INSPECTOR_TABS];

export function isSessionTab(value: string | undefined): SessionTab | null {
	return value && (DETAIL_TABS as string[]).includes(value)
		? (value as SessionTab)
		: null;
}

/** Every tab reads one named source; several tabs share a source. */
export type TabDataKey =
	| 'summary'
	| 'graph'
	| 'iterations'
	| 'variables'
	| 'checkpoints'
	| 'timeline'
	| 'messages';

export const TAB_DATA: Record<SessionTab, TabDataKey[]> = {
	overview: ['summary'],
	graph: ['graph'],
	iterations: ['iterations'],
	variables: ['variables'],
	checkpoints: ['checkpoints'],
	tools: ['iterations'],
	timeline: ['timeline'],
	analysis: ['iterations'],
	messages: ['messages'],
};
