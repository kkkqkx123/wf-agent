import { client } from '$lib/api/client';
import { call } from '$lib/api/envelope';

/**
 * Behavioral defaults kept in the server-side preference document so they
 * follow the user across browsers. Look-and-feel (theme, density, shell
 * geometry) has to be applied before first paint and therefore stays in
 * `stores/preferences.svelte.ts`; the two key groups never mix.
 */
export interface Behavior {
	pageSize: number;
	autoRefresh: boolean;
	streamFollow: boolean;
	reduceMotion: boolean;
}

type PreferenceDocument = Record<string, unknown>;

export const DEFAULT_BEHAVIOR: Behavior = {
	pageSize: 50,
	autoRefresh: true,
	streamFollow: true,
	reduceMotion: false,
};

const PAGE_SIZE_OPTION = 'execution.page-size';
const AUTO_REFRESH_OPTION = 'execution.auto-refresh';
const STREAM_FOLLOW_OPTION = 'execution.stream-follow';
const REDUCE_MOTION_OPTION = 'appearance.reduce-motion';

function booleanOption(
	doc: PreferenceDocument,
	key: string,
	fallback: boolean,
): boolean {
	const value = doc[key];
	return typeof value === 'boolean' ? value : fallback;
}

function fromDocument(doc: PreferenceDocument): Behavior {
	const pageSize = doc[PAGE_SIZE_OPTION];
	return {
		pageSize:
			typeof pageSize === 'number' && Number.isFinite(pageSize)
				? pageSize
				: DEFAULT_BEHAVIOR.pageSize,
		autoRefresh: booleanOption(
			doc,
			AUTO_REFRESH_OPTION,
			DEFAULT_BEHAVIOR.autoRefresh,
		),
		streamFollow: booleanOption(
			doc,
			STREAM_FOLLOW_OPTION,
			DEFAULT_BEHAVIOR.streamFollow,
		),
		reduceMotion: booleanOption(
			doc,
			REDUCE_MOTION_OPTION,
			DEFAULT_BEHAVIOR.reduceMotion,
		),
	};
}

function toDocument(behavior: Behavior): PreferenceDocument {
	return {
		[PAGE_SIZE_OPTION]: behavior.pageSize,
		[AUTO_REFRESH_OPTION]: behavior.autoRefresh,
		[STREAM_FOLLOW_OPTION]: behavior.streamFollow,
		[REDUCE_MOTION_OPTION]: behavior.reduceMotion,
	};
}

export async function loadBehavior(): Promise<Behavior> {
	const doc = await call<PreferenceDocument>(client.GET('/api/v1/preferences'));
	return fromDocument(doc);
}

/** Write the whole document back; the server replaces it verbatim. */
export async function saveBehavior(behavior: Behavior): Promise<void> {
	await call<unknown>(
		client.PUT('/api/v1/preferences', {
			body: { values: toDocument(behavior) },
		}),
	);
}
