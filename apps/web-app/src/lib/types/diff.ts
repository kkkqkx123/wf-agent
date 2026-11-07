/**
 * Single diff view model for the frontend. Both file-checkpoint diffs
 * (unified text from `FileDiffDoc`) and user-repo git diffs (hunk rows)
 * are adapted to this shape at the service boundary, so `DiffView`
 * only ever receives one input type.
 */

export interface DiffLine {
	type: 'add' | 'del' | 'context' | 'meta';
	text: string;
}

export interface DiffFile {
	path: string;
	kind: string;
	lines: DiffLine[];
	additions: number;
	deletions: number;
	binary: boolean;
	truncated: boolean;
}

/** Rows rendered per diff; longer diffs are cut with a truncation flag. */
const DIFF_LINE_LIMIT = 400;

// TODO (backend): align the checkpoint and git diff API schemas on one
// structured shape (path, binary/truncation flags, typed line rows).
// Until then this parser bridges the unified-text output; it lives
// here so both services share one copy.
export function parseUnifiedDiff(text: string): {
	lines: DiffLine[];
	truncated: boolean;
} {
	const rows = text.split('\n');
	const lines: DiffLine[] = [];
	for (const raw of rows) {
		if (lines.length >= DIFF_LINE_LIMIT) break;
		if (
			raw.startsWith('+++') ||
			raw.startsWith('---') ||
			raw.startsWith('diff ') ||
			raw.startsWith('index ') ||
			raw.startsWith('@@')
		) {
			lines.push({ type: 'meta', text: raw });
		} else if (raw.startsWith('+')) {
			lines.push({ type: 'add', text: raw.slice(1) });
		} else if (raw.startsWith('-')) {
			lines.push({ type: 'del', text: raw.slice(1) });
		} else if (raw.startsWith('\\')) {
			continue;
		} else {
			lines.push({
				type: 'context',
				text: raw.startsWith(' ') ? raw.slice(1) : raw,
			});
		}
	}
	return { lines, truncated: rows.length > lines.length };
}

interface UnifiedDiffDoc {
	path: string;
	kind: string;
	diff?: string | null;
	additions?: number | null;
	deletions?: number | null;
}

/** Adapt a `FileDiffDoc`-shaped hunk row set to the shared view model. */
export function fileDiffDocToDiffFile(d: {
	path: string;
	kind: string;
	hunks: { header: string; lines: string[] }[];
	additions: number;
	deletions: number;
	binary: boolean;
	truncated: boolean;
}): DiffFile {
	if (d.binary) {
		return {
			path: d.path,
			kind: d.kind.toLowerCase(),
			lines: [],
			additions: d.additions,
			deletions: d.deletions,
			binary: true,
			truncated: d.truncated,
		};
	}
	const lines: DiffLine[] = [];
	let truncated = d.truncated;
	for (const hunk of d.hunks) {
		if (lines.length >= DIFF_LINE_LIMIT) {
			truncated = true;
			break;
		}
		lines.push({ type: 'meta', text: hunk.header });
		for (const raw of hunk.lines) {
			if (lines.length >= DIFF_LINE_LIMIT) {
				truncated = true;
				break;
			}
			if (raw.startsWith('+')) lines.push({ type: 'add', text: raw.slice(1) });
			else if (raw.startsWith('-')) lines.push({ type: 'del', text: raw.slice(1) });
			else
				lines.push({
					type: 'context',
					text: raw.startsWith(' ') ? raw.slice(1) : raw,
				});
		}
	}
	return {
		path: d.path,
		kind: d.kind.toLowerCase(),
		lines,
		additions: d.additions,
		deletions: d.deletions,
		binary: false,
		truncated,
	};
}

/** Adapt a `FileDiffDoc` (unified diff text) to the shared view model. */
export function toDiffFile(d: UnifiedDiffDoc): DiffFile {
	const text = d.diff ?? '';
	const parsed = parseUnifiedDiff(text);
	return {
		path: d.path,
		kind: d.kind.toLowerCase(),
		lines: parsed.lines,
		additions: d.additions ?? 0,
		deletions: d.deletions ?? 0,
		binary: !text,
		truncated: parsed.truncated,
	};
}

export interface DiffHunkRow {
	header: string;
	lines: string[];
}

/** Adapt git hunk rows to the shared view model. */
export function gitHunksToDiffFile(
	path: string,
	hunks: DiffHunkRow[],
	additions: number,
	deletions: number,
): DiffFile {
	const lines: DiffLine[] = [];
	let truncated = false;
	for (const hunk of hunks) {
		if (lines.length >= DIFF_LINE_LIMIT) {
			truncated = true;
			break;
		}
		lines.push({ type: 'meta', text: hunk.header });
		for (const raw of hunk.lines) {
			if (lines.length >= DIFF_LINE_LIMIT) {
				truncated = true;
				break;
			}
			if (raw.startsWith('+')) lines.push({ type: 'add', text: raw.slice(1) });
			else if (raw.startsWith('-')) lines.push({ type: 'del', text: raw.slice(1) });
			else
				lines.push({
					type: 'context',
					text: raw.startsWith(' ') ? raw.slice(1) : raw,
				});
		}
	}
	return {
		path,
		kind: 'modified',
		lines,
		additions,
		deletions,
		binary: false,
		truncated,
	};
}

/** Whether resolved text still carries diff3 conflict markers. */
export function hasConflictMarkers(text: string): boolean {
	return text.includes('<<<<<<<') || text.includes('=======') || text.includes('>>>>>>>');
}

/** Prefix used when a rendered row is serialized back to plain text. */
export function diffLinePrefix(type: DiffLine['type']): string {
	if (type === 'add') return '+';
	if (type === 'del') return '-';
	if (type === 'context') return ' ';
	return '';
}

/** Serialize one hunk (header plus content rows) to plain text. */
export function diffHunkToText(header: string, rows: DiffLine[]): string {
	const out = [header];
	for (const row of rows) {
		if (row.type === 'meta') out.push(row.text);
		else out.push(`${diffLinePrefix(row.type)}${row.text}`);
	}
	return out.join('\n');
}

/**
 * Serialize one rendered file block to plain text. The optional title is
 * the same heading the diff view shows, so pasted text matches the screen.
 * Binary blocks carry a placeholder row, truncated blocks end with a
 * marker row, keeping the output honest about what was actually shown.
 */
export function diffFileToText(file: DiffFile, title?: string): string {
	const out: string[] = [];
	if (title) out.push(title);
	if (file.binary && file.lines.length === 0) {
		out.push('(binary file, no text diff)');
		return out.join('\n');
	}
	for (const line of file.lines) {
		if (line.type === 'meta') out.push(line.text);
		else out.push(`${diffLinePrefix(line.type)}${line.text}`);
	}
	if (file.truncated) out.push('... (truncated, not all lines shown)');
	return out.join('\n');
}

/** Serialize several rendered file blocks, separated by a blank line. */
export function diffFilesToText(blocks: Array<{ title?: string; file: DiffFile }>): string {
	return blocks.map((block) => diffFileToText(block.file, block.title)).join('\n\n');
}
