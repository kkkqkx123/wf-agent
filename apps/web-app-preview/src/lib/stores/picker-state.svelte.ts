import { browser } from '$app/environment';
import { listFavorites, setFavorite } from '$lib/services/favorites';
import { toasts } from '$lib/stores/toast.svelte';

/** Kinds of instances the composer lets the user pick. */
export type PickerKind = 'agent' | 'model' | 'workflow';

const RECENTS_KEY = 'wf-picker-recents';

/** How many recently used entries one kind keeps. */
export const PICKER_RECENT_LIMIT = 6;

function readRecents(): Record<string, string[]> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(RECENTS_KEY);
		return raw ? (JSON.parse(raw) as Record<string, string[]>) : {};
	} catch {
		return {};
	}
}

function writeRecents(value: Record<string, string[]>): void {
	if (!browser) return;
	try {
		localStorage.setItem(RECENTS_KEY, JSON.stringify(value));
	} catch {
		// Storage may be unavailable; the store still works in memory.
	}
}

/**
 * Pinned and recently used instances of the composer pickers.
 *
 * Pins live on the server (the favorites service already stores a pinned
 * flag per resource), so a pin survives a restart and follows the user
 * across browsers. Recency is a local convention: the server models no
 * per-user pick history, and the composer only needs the last few choices.
 */
class PickerState {
	/** Pinned ids per kind, most recently pinned first. */
	private pinned = $state<Record<string, string[]>>({});
	/** Recently used ids per kind, most recent first. */
	private recents = $state<Record<string, string[]>>(readRecents());
	private loaded = false;

	/** Load the pinned entries of every picker kind. */
	async load(): Promise<void> {
		if (this.loaded) return;
		this.loaded = true;
		try {
			const page = await listFavorites({ limit: 200 });
			const byKind: Record<string, string[]> = {};
			for (const item of page.items) {
				if (!item.pinned) continue;
				const bucket = byKind[item.kind] ?? [];
				bucket.push(item.id);
				byKind[item.kind] = bucket;
			}
			this.pinned = byKind;
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : String(e));
		}
	}

	pinnedIds(kind: PickerKind): string[] {
		return this.pinned[kind] ?? [];
	}

	recentIds(kind: PickerKind): string[] {
		return this.recents[kind] ?? [];
	}

	/** Pin or unpin an entry; the server owns the pin, so the change waits
	 * for its answer before the local view moves. */
	async setPinned(kind: PickerKind, id: string, pinned: boolean): Promise<void> {
		const current = this.pinnedIds(kind);
		const next = pinned
			? [id, ...current.filter((entry) => entry !== id)]
			: current.filter((entry) => entry !== id);
		this.pinned = { ...this.pinned, [kind]: next };
		try {
			await setFavorite(kind, id, { pinned });
		} catch (e) {
			this.pinned = { ...this.pinned, [kind]: current };
			toasts.error(e instanceof Error ? e.message : String(e));
		}
	}

	/** Remember an entry as recently used. */
	recordUsed(kind: PickerKind, id: string): void {
		if (!id) return;
		const current = this.recents[kind] ?? [];
		const next = [id, ...current.filter((entry) => entry !== id)].slice(
			0,
			PICKER_RECENT_LIMIT,
		);
		this.recents = { ...this.recents, [kind]: next };
		writeRecents(this.recents);
	}
}

export const pickerState = new PickerState();
