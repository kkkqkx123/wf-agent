import { listAgentLoops } from '$lib/services/agent-loops';
import {
	listFavorites,
	removeFavorite,
	setFavorite,
} from '$lib/services/favorites';
import { createCollection } from './collection.svelte';
import { toasts } from './toast.svelte';

const FAVORITE_KIND = 'agent_loop';

function errorMessage(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

/** Remote session state: the loop registry collection and the star service. */
export class SessionRemoteState {
	list = createCollection((params) => listAgentLoops(params));

	starredIds = $state<string[]>([]);
	private loadingStars = false;

	isStarred(id: string): boolean {
		return this.starredIds.includes(id);
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
