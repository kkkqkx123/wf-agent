import { listFsTree, type FsTreeEntry } from '#lib/services/fs';

export interface FsTreeIndex {
	root: string;
	entries: FsTreeEntry[];
	truncated: boolean;
	fetchedAt: number;
}

const indexCache = new Map<string, FsTreeIndex>();

export function cachedFsTreeIndex(root: string): FsTreeIndex | null {
	return indexCache.get(root) ?? null;
}

export async function getFsTreeIndex(root: string): Promise<FsTreeIndex> {
	const cached = indexCache.get(root);
	if (cached) return cached;
	const result = await listFsTree('', 6, 2000);
	const index: FsTreeIndex = {
		root,
		entries: result.entries,
		truncated: result.truncated,
		fetchedAt: Date.now(),
	};
	indexCache.set(root, index);
	return index;
}

/** Drop the cached file index after structural mutations. The cache is keyed
 * by workspace root and the app serves a single workspace, so any
 * create/rename/delete conservatively refreshes on the next palette open. */
export function invalidateFsTreeIndex(): void {
	indexCache.clear();
}

function subsequenceScore(query: string, target: string): number | null {
	let qi = 0;
	let score = 0;
	let lastMatch = -1;
	for (let ti = 0; ti < target.length && qi < query.length; ti += 1) {
		if (target[ti] === query[qi]) {
			score += lastMatch + 1 === ti ? 2 : 1;
			lastMatch = ti;
			qi += 1;
		}
	}
	if (qi < query.length) return null;
	return score;
}

export interface RankedFsEntry extends FsTreeEntry {
	score: number;
}

/** Path-subsequence match with prefix and filename weighting. */
export function filterFsTree(entries: FsTreeEntry[], query: string, limit = 10): RankedFsEntry[] {
	const needle = query.trim().toLowerCase();
	if (needle.length < 2) return [];
	const ranked: RankedFsEntry[] = [];
	for (const entry of entries) {
		const target = entry.path.toLowerCase();
		const fileName = entry.path.split('/').pop()?.toLowerCase() ?? target;
		const base = subsequenceScore(needle, target);
		if (base === null) continue;
		let score = base;
		if (target.startsWith(needle)) score += 50;
		if (fileName.startsWith(needle)) score += 30;
		if (!entry.is_dir) score += 5;
		ranked.push({ ...entry, score });
	}
	ranked.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path));
	return ranked.slice(0, limit);
}
