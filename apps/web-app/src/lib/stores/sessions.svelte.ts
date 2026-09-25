import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { listAgentLoops } from '$lib/services/agent-loops';
import {
	listFavorites,
	removeFavorite,
	setFavorite,
} from '$lib/services/favorites';
import type { AgentLoop } from '$lib/types/models';
import { createCollection } from './collection.svelte';
import { chatStream } from './stream-run.svelte';
import { toasts } from './toast.svelte';

const TITLES_KEY = 'wf-session-titles';
const RECENTS_KEY = 'wf-session-recents';
const DRAFTS_KEY = 'wf-session-drafts';

const RECENT_LIMIT = 12;

/** Draft key for a composer that has not produced a loop yet. */
export const NEW_SESSION = 'new';

const FAVORITE_KIND = 'agent_loop';

function readRecord(key: string): Record<string, string> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as Record<string, string>) : {};
	} catch {
		return {};
	}
}

function readList(key: string): string[] {
	if (!browser) return [];
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as string[]) : [];
	} catch {
		return [];
	}
}

function write(key: string, value: unknown): void {
	if (!browser) return;
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Storage may be unavailable; the store still works in memory.
	}
}

function byNewest(a: AgentLoop, b: AgentLoop): number {
	return a.startedAt < b.startedAt ? 1 : -1;
}

function errorMessage(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

/**
 * The single session state shared by the sidebar, the chat column and the
 * command palette. The loop rows come from the registry; titles, stars,
 * recents and drafts are local conventions the backend does not model.
 */
class SessionStore {
	list = createCollection((params) => listAgentLoops(params));

	starredIds = $state<string[]>([]);
	private titles = $state<Record<string, string>>(readRecord(TITLES_KEY));
	private recents = $state<string[]>(readList(RECENTS_KEY));
	private draftTexts = $state<Record<string, string>>(readRecord(DRAFTS_KEY));
	private loadingStars = false;

	get sessions(): AgentLoop[] {
		return [...this.list.items].sort(byNewest);
	}

	/** Rows the sidebar lists, in the order the local state puts them in. */
	private rowsFor(ids: string[]): AgentLoop[] {
		const byId = new Map(this.list.items.map((item) => [item.id, item]));
		return ids
			.map((id) => byId.get(id))
			.filter((item): item is AgentLoop => item !== undefined);
	}

	get starred(): AgentLoop[] {
		return this.rowsFor(this.starredIds).sort(byNewest);
	}

	get recent(): AgentLoop[] {
		return this.rowsFor(this.recents);
	}

	get draftIds(): string[] {
		return Object.keys(this.draftTexts).filter(
			(id) => id !== NEW_SESSION && this.draftTexts[id].trim() !== '',
		);
	}

	get drafts(): AgentLoop[] {
		return this.rowsFor(this.draftIds).sort(byNewest);
	}

	label(id: string): string {
		return this.titles[id] ?? id;
	}

	/** First user message names the session; a recorded title is never rewritten. */
	recordTitle(id: string, content: string): void {
		if (this.titles[id]) return;
		const line = content.trim().split('\n', 1)[0];
		if (!line) return;
		this.titles = { ...this.titles, [id]: line.slice(0, 60) };
		write(TITLES_KEY, this.titles);
	}

	isStarred(id: string): boolean {
		return this.starredIds.includes(id);
	}

	touch(id: string): void {
		const next = [id, ...this.recents.filter((entry) => entry !== id)];
		this.recents = next.slice(0, RECENT_LIMIT);
		write(RECENTS_KEY, this.recents);
	}

	/**
	 * Open one session in the chat column. The address carries the selection, so
	 * the sidebar, the chat header and a shared link all land on the same session.
	 */
	open(id: string): void {
		this.touch(id);
		// The rule cannot see that the path half of the target is resolved.
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		void goto(`${resolve('/chat')}?id=${encodeURIComponent(id)}`);
	}

	/** Leave any finished run behind and open a fresh draft. */
	startDraft(): void {
		chatStream.stop();
		void goto(resolve('/chat'));
	}

	draft(id: string): string {
		return this.draftTexts[id] ?? '';
	}

	setDraft(id: string, text: string): void {
		const next = { ...this.draftTexts };
		if (text.trim()) next[id] = text;
		else delete next[id];
		this.draftTexts = next;
		write(DRAFTS_KEY, next);
	}

	async refresh(): Promise<void> {
		await Promise.all([this.list.reload(), this.refreshStars()]);
	}

	async refreshStars(): Promise<void> {
		if (this.loadingStars) return;
		this.loadingStars = true;
		try {
			const page = await listFavorites({ kind: FAVORITE_KIND, limit: 200 });
			this.starredIds = page.items.map((item) => item.id);
		} catch (e) {
			toasts.error(errorMessage(e));
		} finally {
			this.loadingStars = false;
		}
	}

	async toggleStar(id: string): Promise<void> {
		const starred = this.isStarred(id);
		this.starredIds = starred
			? this.starredIds.filter((entry) => entry !== id)
			: [...this.starredIds, id];
		try {
			if (starred) await removeFavorite(FAVORITE_KIND, id);
			else await setFavorite(FAVORITE_KIND, id, {});
		} catch (e) {
			this.starredIds = starred
				? [...this.starredIds, id]
				: this.starredIds.filter((entry) => entry !== id);
			toasts.error(errorMessage(e));
		}
	}
}

export const sessions = new SessionStore();
