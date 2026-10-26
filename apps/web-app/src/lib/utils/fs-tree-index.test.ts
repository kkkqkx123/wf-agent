import { describe, expect, it } from 'vitest';
import { filterFsTree } from './fs-tree-index';

const entries = [
	{ path: 'src/lib/utils/mentions.ts', is_dir: false, size: 10 },
	{ path: 'src/lib/utils/ide-url.ts', is_dir: false, size: 10 },
	{ path: 'src/lib/stores/ide.svelte.ts', is_dir: false, size: 10 },
	{ path: 'docs/plan/ide.md', is_dir: false, size: 10 },
	{ path: 'src/lib', is_dir: true, size: 0 },
];

describe('filterFsTree', () => {
	it('requires a minimum query length', () => {
		expect(filterFsTree(entries, 'a')).toEqual([]);
	});

	it('prefers filename prefix matches', () => {
		const [first] = filterFsTree(entries, 'ide-u');
		expect(first?.path).toBe('src/lib/utils/ide-url.ts');
	});

	it('matches path subsequences', () => {
		const paths = filterFsTree(entries, 'men').map((e) => e.path);
		expect(paths).toContain('src/lib/utils/mentions.ts');
	});

	it('caps results at the requested limit', () => {
		expect(filterFsTree(entries, 'src', 1)).toHaveLength(1);
	});
});
