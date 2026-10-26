import { badgeForFile } from '#lib/utils/git-badges';
import type { BranchEntry, CommitEntry, GitFile } from '#lib/services/git';

/** Row text tone for a file entry, following staged-first coloring. */
export function fileRowClass(
	file: Pick<GitFile, 'staged' | 'unstaged' | 'kind' | 'renamedFrom'>,
): string {
	const level = badgeForFile(file);
	switch (level) {
		case 'conflicted':
			return 'text-destructive font-semibold';
		case 'added':
			return 'text-success';
		case 'deleted':
			return 'text-destructive';
		case 'renamed':
			return 'text-info';
		case 'untracked':
			return 'text-muted-foreground';
		case 'modified':
			return file.unstaged !== '.' && file.unstaged !== '?' ? 'text-warning' : 'text-success';
		default:
			return 'text-foreground';
	}
}

/** Single-letter status marker tone for the staged or unstaged column. */
export function markerClass(marker: string, side: 'staged' | 'unstaged'): string {
	if (marker === '.' || marker === ' ') return 'text-muted-foreground/50';
	if (marker === '?') return 'text-muted-foreground';
	if (marker === 'U') return 'text-destructive font-bold';
	if (side === 'staged') {
		if (marker === 'D') return 'text-destructive';
		return 'text-success';
	}
	if (marker === 'D' || marker === 'M' || marker === 'A') return 'text-warning';
	return 'text-warning';
}

/** Branch rows highlight the checked-out branch. */
export function branchRowClass(branch: Pick<BranchEntry, 'current'>): string {
	return branch.current ? 'text-warning font-semibold' : 'text-foreground';
}

/** Commit rows highlight refs that point at the checked-out head. */
export function commitRowClass(commit: Pick<CommitEntry, 'refs'>): string {
	return commit.refs.some((ref) => ref.includes('HEAD')) ? 'text-warning' : 'text-foreground';
}

/** Stash rows carry no state tone. */
export function stashRowClass(): string {
	return 'text-foreground';
}
