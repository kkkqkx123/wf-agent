import { describe, expect, it, vi } from 'vitest';

// The route module pulls in SvelteKit's browser-only modules for `goto`; the
// helpers under test are pure.
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ base: '', resolve: (path: string) => path }));

const { parseListParams, buildListQuery } = await import('./route');

function url(search: string): URL {
	return new URL(`https://app.test/executions${search}`);
}

describe('parseListParams', () => {
	it('keeps only the list keys that carry a value', () => {
		expect(
			parseListParams(url('?q=node&status=running&other=3&page=2#frag')),
		).toEqual({ q: 'node', status: 'running', page: '2' });
	});

	it('treats an empty value as absent', () => {
		expect(parseListParams(url('?q=&id=x'))).toEqual({ id: 'x' });
	});
});

describe('buildListQuery', () => {
	it('merges a patch into the existing query, unknown keys included', () => {
		expect(buildListQuery(url('?status=running&z=1'), { q: 'abc' })).toBe(
			'status=running&z=1&q=abc',
		);
	});

	it('drops a key patched with an empty value', () => {
		expect(buildListQuery(url('?q=abc&page=3'), { q: '' })).toBe('page=3');
	});

	it('ignores keys it does not own', () => {
		expect(buildListQuery(url('?q=abc'), { other: 'x' } as never)).toBe(
			'q=abc',
		);
	});

	it('is stable when re-applied, which is what stops write-back loops', () => {
		const once = buildListQuery(url('?q=abc'), { q: 'abc' });
		expect(buildListQuery(url(`?${once}`), { q: 'abc' })).toBe(once);
	});
});
