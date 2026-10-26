import { describe, expect, it } from 'vitest';
import { branchRowClass, commitRowClass, fileRowClass, markerClass } from './tone';

describe('fileRowClass', () => {
	it('marks conflicted rows destructive and bold', () => {
		expect(
			fileRowClass({ staged: 'U', unstaged: 'U', kind: 'conflicted', renamedFrom: null }),
		).toBe('text-destructive font-semibold');
	});

	it('marks staged additions green and unstaged modifications amber', () => {
		expect(fileRowClass({ staged: 'A', unstaged: '.', kind: 'staged', renamedFrom: null })).toBe(
			'text-success',
		);
		expect(fileRowClass({ staged: '.', unstaged: 'M', kind: 'unstaged', renamedFrom: null })).toBe(
			'text-warning',
		);
	});

	it('marks untracked rows muted', () => {
		expect(fileRowClass({ staged: '?', unstaged: '?', kind: 'untracked', renamedFrom: null })).toBe(
			'text-muted-foreground',
		);
	});
});

describe('markerClass', () => {
	it('dims clean markers and colors staged additions green', () => {
		expect(markerClass('.', 'staged')).toBe('text-muted-foreground/50');
		expect(markerClass('A', 'staged')).toBe('text-success');
		expect(markerClass('M', 'unstaged')).toBe('text-warning');
	});
});

describe('branchRowClass', () => {
	it('highlights the current branch', () => {
		expect(branchRowClass({ current: true })).toBe('text-warning font-semibold');
		expect(branchRowClass({ current: false })).toBe('text-foreground');
	});
});

describe('commitRowClass', () => {
	it('highlights head commits', () => {
		expect(commitRowClass({ refs: ['HEAD -> main'] })).toBe('text-warning');
		expect(commitRowClass({ refs: [] })).toBe('text-foreground');
	});
});
