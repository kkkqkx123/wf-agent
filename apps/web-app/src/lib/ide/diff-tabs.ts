import type { DiffFile } from '#lib/types/diff';

/** Comparison base for an IDE diff tab. Only the working-tree side is
 * ever editable; staged snapshots and commit ranges stay read-only. */
export type DiffBase =
	{ kind: 'unstaged' } | { kind: 'staged' } | { kind: 'commit'; commit: string; parent: string };

export interface DiffTarget {
	path: string;
	base: DiffBase;
}

/** Stable tab key: one tab per file per base. */
export function diffKeyFor(target: DiffTarget): string {
	switch (target.base.kind) {
		case 'unstaged':
			return `unstaged:${target.path}`;
		case 'staged':
			return `staged:${target.path}`;
		case 'commit':
			return `commit:${target.base.commit}:${target.path}`;
	}
}

/** Whether the modified side of this diff may be edited. */
export function diffEditable(base: DiffBase): boolean {
	return base.kind === 'unstaged';
}

/** Well-known empty tree object: the parent of a root commit. */
export const EMPTY_TREE_SHA = '4b825dc642cb6eb9a060e54bf8d69288fbee4904';

export function fileNameOf(path: string): string {
	return path.split('/').pop() ?? path;
}
