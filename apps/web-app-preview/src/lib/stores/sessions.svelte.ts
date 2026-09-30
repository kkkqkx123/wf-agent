import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { AgentLoop } from '$lib/types/models';
import { NEW_SESSION, SessionLocalState } from './session-local.svelte';
import { SessionRemoteState } from './session-remote.svelte';
import { chatStream } from './stream-run.svelte';

export { NEW_SESSION };

function byNewest(a: AgentLoop, b: AgentLoop): number {
	return a.startedAt < b.startedAt ? 1 : -1;
}

/**
 * The single session state shared by the sidebar, the chat column and the
 * command palette. Remote loop rows and stars come from the backend while
 * titles, recents and drafts are local conventions; navigation keeps the
 * address as the shared selection.
 */
class SessionStore {
	private readonly remote = new SessionRemoteState();
	private readonly local = new SessionLocalState();

	get list() {
		return this.remote.list;
	}

	get starredIds(): string[] {
		return this.remote.starredIds;
	}

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
		return this.rowsFor(this.local.recentIds);
	}

	get draftIds(): string[] {
		return this.local.draftIds;
	}

	get drafts(): AgentLoop[] {
		return this.rowsFor(this.draftIds).sort(byNewest);
	}

	label(id: string): string {
		return this.local.label(id);
	}

	/** First user message names the session; a recorded title is never rewritten. */
	recordTitle(id: string, content: string): void {
		this.local.recordTitle(id, content);
	}

	isStarred(id: string): boolean {
		return this.remote.isStarred(id);
	}

	touch(id: string): void {
		this.local.touch(id);
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
		return this.local.draft(id);
	}

	setDraft(id: string, text: string): void {
		this.local.setDraft(id, text);
	}

	async refresh(): Promise<void> {
		await this.remote.refresh();
	}

	async refreshStars(): Promise<void> {
		await this.remote.refreshStars();
	}

	async toggleStar(id: string): Promise<void> {
		await this.remote.toggleStar(id);
	}
}

export const sessions = new SessionStore();
