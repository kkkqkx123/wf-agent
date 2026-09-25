import { beforeEach, describe, expect, it } from 'vitest';
import { createCollection, createResource } from './collection.svelte';
import type { PageParams } from './collection.svelte';
import type { PageResult } from '$lib/api/envelope';
import { behavior } from './behavior.svelte';

interface Row {
	id: string;
}

function page(items: Row[], hasMore: boolean): PageResult<Row> {
	return { items, hasMore, limit: behavior.pageSize, offset: 0 };
}

beforeEach(() => {
	behavior.pageSize = 2;
});

describe('Collection', () => {
	it('reloads from offset zero and records hasMore', async () => {
		const calls: PageParams[] = [];
		const list = createCollection<Row>((params) => {
			calls.push(params);
			return Promise.resolve(page([{ id: 'a' }, { id: 'b' }], true));
		});

		await list.reload();

		expect(calls).toEqual([{ limit: 2, offset: 0 }]);
		expect(list.items.map((item) => item.id)).toEqual(['a', 'b']);
		expect(list.hasMore).toBe(true);
		expect(list.pageSize).toBe(2);
		expect(list.loading).toBe(false);
		expect(list.error).toBeNull();
	});

	it('loadMore appends the next window and drops duplicate ids', async () => {
		const offsets: number[] = [];
		const list = createCollection<Row>((params) => {
			offsets.push(params.offset);
			if (params.offset === 0)
				return Promise.resolve(page([{ id: 'a' }], true));
			return Promise.resolve(page([{ id: 'a' }, { id: 'b' }], false));
		});

		await list.reload();
		await list.loadMore();

		expect(offsets).toEqual([0, 1]);
		expect(list.items.map((item) => item.id)).toEqual(['a', 'b']);
		expect(list.hasMore).toBe(false);
	});

	it('walks the cursor forward to reach a page number from the URL', async () => {
		const offsets: number[] = [];
		const list = createCollection<Row>((params) => {
			offsets.push(params.offset);
			return Promise.resolve(
				page([{ id: `row-${params.offset}` }], params.offset < 3),
			);
		});

		await list.loadPages(3);

		expect(offsets).toEqual([0, 1, 2]);
		expect(list.items.map((item) => item.id)).toEqual([
			'row-0',
			'row-1',
			'row-2',
		]);
	});

	it('surfaces fetch failures and clears them on the next reload', async () => {
		let fail = true;
		const list = createCollection<Row>(() => {
			if (fail) return Promise.reject(new Error('boom'));
			return Promise.resolve(page([{ id: 'a' }], false));
		});

		await list.reload();
		expect(list.error).toBe('boom');
		expect(list.items).toEqual([]);

		fail = false;
		await list.reload();
		expect(list.error).toBeNull();
		expect(list.loaded).toBe(1);
	});

	it('ignores a stale response when a newer reload starts', async () => {
		const ready: ((value: PageResult<Row>) => void)[] = [];
		const list = createCollection<Row>(() => {
			return new Promise((resolve) => ready.push(resolve));
		});

		const firstAttempt = list.reload();
		const secondAttempt = list.reload();

		ready[1](page([{ id: 'newer' }], false));
		await secondAttempt;
		ready[0](page([{ id: 'stale' }], false));
		await firstAttempt;

		expect(list.items.map((item) => item.id)).toEqual(['newer']);
		expect(list.loading).toBe(false);
	});
});

describe('Resource', () => {
	it('stores the value and reports loading transitions', async () => {
		const resource = createResource(() => Promise.resolve({ id: 'x' }));
		expect(resource.data).toBeNull();

		const pending = resource.reload();
		expect(resource.loading).toBe(true);
		await pending;

		expect(resource.data).toEqual({ id: 'x' });
		expect(resource.loading).toBe(false);
	});

	it('keeps the last value while a refresh is in flight and reports errors', async () => {
		let fail = false;
		const resource = createResource<{ id: string }>(() => {
			if (fail) return Promise.reject(new Error('down'));
			return Promise.resolve({ id: 'x' });
		});

		await resource.reload();
		fail = true;
		await resource.reload();

		expect(resource.data).toEqual({ id: 'x' });
		expect(resource.error).toBe('down');
	});
});
