import type { GitFile } from '#lib/services/git';

export type GitBadgeLevel =
	| 'added'
	| 'modified'
	| 'deleted'
	| 'renamed'
	| 'conflicted'
	| 'untracked';

const SEVERITY: Record<GitBadgeLevel, number> = {
	conflicted: 6,
	renamed: 5,
	deleted: 4,
	added: 3,
	modified: 2,
	untracked: 1,
};

/** Map one git change row to its badge level. Clean rows resolve to null. */
export function badgeForFile(file: Pick<GitFile, 'staged' | 'unstaged' | 'kind' | 'renamedFrom'>): GitBadgeLevel | null {
	const { staged, unstaged, kind } = file;
	if (
		kind === 'conflicted' ||
		staged === 'U' ||
		unstaged === 'U' ||
		(staged === 'A' && unstaged === 'A')
	) {
		return 'conflicted';
	}
	if (file.renamedFrom || kind === 'renamed') return 'renamed';
	if (staged === '?' || unstaged === '?' || kind === 'untracked') return 'untracked';
	if (staged === 'D' || unstaged === 'D') return 'deleted';
	if (staged === 'A' || unstaged === 'A') return 'added';
	if (staged !== '.' || unstaged !== '.' || kind === 'staged' || kind === 'unstaged') {
		return 'modified';
	}
	return null;
}

/** Aggregate descendant levels to the most severe one for directories. */
export function aggregateBadges(levels: Iterable<GitBadgeLevel | null | undefined>): GitBadgeLevel | null {
	let best: GitBadgeLevel | null = null;
	for (const level of levels) {
		if (!level) continue;
		if (!best || SEVERITY[level] > SEVERITY[best]) best = level;
	}
	return best;
}
