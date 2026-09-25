import type { PageResult } from '$lib/api/envelope';
import { behavior } from './behavior.svelte';

export interface PageParams {
	limit: number;
	offset: number;
}

function errorMessage(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

function itemId(item: unknown): string {
	const id = (item as { id?: unknown }).id;
	return typeof id === 'string' ? id : '';
}

/**
 * Paginated list state: offset-based reload/loadMore with real loading and
 * error flags, safe against overlapping reload/loadMore races.
 */
export class Collection<T> {
	items = $state<T[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);
	hasMore = $state(false);

	private readonly fetcher: (params: PageParams) => Promise<PageResult<T>>;
	private request = 0;

	constructor(fetcher: (params: PageParams) => Promise<PageResult<T>>) {
		this.fetcher = fetcher;
	}

	/** Page size follows the shared preference document. */
	get pageSize(): number {
		return behavior.pageSize;
	}

	get loaded(): number {
		return this.items.length;
	}

	async reload(): Promise<void> {
		const request = ++this.request;
		this.loading = true;
		this.error = null;
		try {
			const page = await this.fetcher({ limit: this.pageSize, offset: 0 });
			if (request !== this.request) return;
			this.items = page.items;
			this.hasMore = page.hasMore;
		} catch (e) {
			if (request !== this.request) return;
			this.error = errorMessage(e);
		} finally {
			if (request === this.request) this.loading = false;
		}
	}

	/** Restore a page count recorded in the URL by walking the offset cursor forward. */
	async loadPages(pages: number): Promise<void> {
		await this.reload();
		for (let index = 1; index < pages && this.hasMore; index += 1) {
			await this.loadMore();
		}
	}

	async loadMore(): Promise<void> {
		if (this.loading || !this.hasMore) return;
		const request = ++this.request;
		this.loading = true;
		this.error = null;
		try {
			const page = await this.fetcher({
				limit: this.pageSize,
				offset: this.items.length,
			});
			if (request !== this.request) return;
			const known = this.items.map(itemId);
			const fresh = page.items.filter((item) => !known.includes(itemId(item)));
			this.items = [...this.items, ...fresh];
			this.hasMore = page.hasMore;
		} catch (e) {
			if (request !== this.request) return;
			this.error = errorMessage(e);
		} finally {
			if (request === this.request) this.loading = false;
		}
	}
}

/** Single-resource state for detail pages. */
export class Resource<T> {
	data = $state.raw<T | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);

	private readonly fetcher: () => Promise<T>;
	private request = 0;

	constructor(fetcher: () => Promise<T>) {
		this.fetcher = fetcher;
	}

	async reload(): Promise<void> {
		const request = ++this.request;
		this.loading = true;
		this.error = null;
		try {
			const data = await this.fetcher();
			if (request !== this.request) return;
			this.data = data;
		} catch (e) {
			if (request !== this.request) return;
			this.error = errorMessage(e);
		} finally {
			if (request === this.request) this.loading = false;
		}
	}
}

export function createCollection<T>(
	fetcher: (params: PageParams) => Promise<PageResult<T>>,
): Collection<T> {
	return new Collection(fetcher);
}

export function createResource<T>(fetcher: () => Promise<T>): Resource<T> {
	return new Resource(fetcher);
}
