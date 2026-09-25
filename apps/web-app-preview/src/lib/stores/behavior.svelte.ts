import {
	DEFAULT_BEHAVIOR,
	loadBehavior,
	saveBehavior,
} from '$lib/services/preferences';
import type { Behavior } from '$lib/services/preferences';

/**
 * Behavioral defaults shared with the backend preference document. The fields
 * start at their defaults so the UI is usable before the document arrives.
 */
class BehaviorStore {
	pageSize = $state(DEFAULT_BEHAVIOR.pageSize);
	autoRefresh = $state(DEFAULT_BEHAVIOR.autoRefresh);
	streamFollow = $state(DEFAULT_BEHAVIOR.streamFollow);
	reduceMotion = $state(DEFAULT_BEHAVIOR.reduceMotion);

	saving = $state(false);
	error = $state<string | null>(null);

	async load(): Promise<void> {
		try {
			const stored = await loadBehavior();
			this.pageSize = stored.pageSize;
			this.autoRefresh = stored.autoRefresh;
			this.streamFollow = stored.streamFollow;
			this.reduceMotion = stored.reduceMotion;
			this.error = null;
		} catch (e) {
			this.error = errorMessage(e);
		}
	}

	async save(): Promise<void> {
		this.saving = true;
		try {
			await saveBehavior(this.current());
			this.error = null;
		} catch (e) {
			this.error = errorMessage(e);
		} finally {
			this.saving = false;
		}
	}

	async restoreDefaults(): Promise<void> {
		this.pageSize = DEFAULT_BEHAVIOR.pageSize;
		this.autoRefresh = DEFAULT_BEHAVIOR.autoRefresh;
		this.streamFollow = DEFAULT_BEHAVIOR.streamFollow;
		this.reduceMotion = DEFAULT_BEHAVIOR.reduceMotion;
		await this.save();
	}

	private current(): Behavior {
		return {
			pageSize: this.pageSize,
			autoRefresh: this.autoRefresh,
			streamFollow: this.streamFollow,
			reduceMotion: this.reduceMotion,
		};
	}
}

function errorMessage(e: unknown): string {
	return e instanceof Error ? e.message : 'Preference request failed';
}

export const behavior = new BehaviorStore();
