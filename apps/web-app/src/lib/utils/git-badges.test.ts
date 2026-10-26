import { describe, expect, it } from 'vitest';
import { aggregateBadges, badgeForFile } from './git-badges';

describe('badgeForFile', () => {
	it('maps conflict markers to conflicted', () => {
		expect(
			badgeForFile({ staged: 'U', unstaged: 'U', kind: 'conflicted', renamedFrom: null }),
		).toBe('conflicted');
	});

	it('maps renames to renamed', () => {
		expect(
			badgeForFile({ staged: 'R', unstaged: '.', kind: 'renamed', renamedFrom: 'a.ts' }),
		).toBe('renamed');
	});

	it('maps untracked rows', () => {
		expect(badgeForFile({ staged: '?', unstaged: '?', kind: 'untracked', renamedFrom: null })).toBe(
			'untracked',
		);
	});

	it('distinguishes added, deleted and modified', () => {
		expect(badgeForFile({ staged: 'A', unstaged: '.', kind: 'staged', renamedFrom: null })).toBe(
			'added',
		);
		expect(badgeForFile({ staged: '.', unstaged: 'D', kind: 'unstaged', renamedFrom: null })).toBe(
			'deleted',
		);
		expect(badgeForFile({ staged: 'M', unstaged: '.', kind: 'staged', renamedFrom: null })).toBe(
			'modified',
		);
	});
});

describe('aggregateBadges', () => {
	it('picks the most severe descendant level', () => {
		expect(aggregateBadges(['modified', 'untracked', 'deleted'])).toBe('deleted');
		expect(aggregateBadges(['untracked', 'renamed'])).toBe('renamed');
		expect(aggregateBadges([null, undefined])).toBeNull();
	});
});
