import { describe, expect, it } from 'vitest';
import { hunksOfDiffFile, selectableIndices } from './workbench.svelte';
import type { DiffFile } from '#lib/types/diff';

function fileWith(lines: DiffFile['lines']): DiffFile {
	return {
		path: 'a.txt',
		kind: 'modified',
		lines,
		additions: 0,
		deletions: 0,
		binary: false,
		truncated: false,
	};
}

describe('staging selection helpers', () => {
	it('splits flattened rows back into per-hunk content rows', () => {
		const file = fileWith([
			{ type: 'meta', text: '@@ -1,2 +1,2 @@' },
			{ type: 'context', text: 'ctx' },
			{ type: 'del', text: 'old' },
			{ type: 'meta', text: '@@ -10,2 +10,2 @@' },
			{ type: 'add', text: 'new' },
		]);
		const hunks = hunksOfDiffFile(file);
		expect(hunks.length).toBe(2);
		expect(hunks[0].length).toBe(2);
		expect(hunks[1].length).toBe(1);
	});

	it('marks only added or removed rows as selectable', () => {
		const rows = [
			{ type: 'context', text: 'ctx' },
			{ type: 'del', text: 'old' },
			{ type: 'add', text: 'new' },
		] as DiffFile['lines'];
		expect(selectableIndices(rows)).toEqual([1, 2]);
	});

	it('returns no hunks for binary views', () => {
		const file = fileWith([]);
		file.binary = true;
		expect(hunksOfDiffFile(file)).toEqual([]);
	});
});
