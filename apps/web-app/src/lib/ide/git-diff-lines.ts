import type { DiffFile } from '#lib/types/diff';

export interface GutterMark {
	line: number;
	kind: 'add' | 'change' | 'delete';
	hover: string;
}

export interface GitLineMap {
	marks: GutterMark[];
	changeLines: number[];
	overflow: boolean;
}

const MAX_MARKS = 200;
const HOVER_SNIPPET_LINES = 8;

const HUNK_HEADER = /@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

interface HunkCursor {
	oldLine: number;
	newLine: number;
	snippet: string[];
}

/** Map unified diff rows onto new-file line numbers for gutter decorations. */
export function diffToLineMap(files: DiffFile[]): GitLineMap {
	const marksByLine = new Map<number, GutterMark>();
	let overflow = false;
	for (const file of files) {
		let cursor: HunkCursor | null = null;
		for (const row of file.lines) {
			if (row.type === 'meta') {
				const match = HUNK_HEADER.exec(row.text);
				cursor = match
					? {
							oldLine: Number.parseInt(match[1] ?? '1', 10),
							newLine: Number.parseInt(match[3] ?? '1', 10),
							snippet: [row.text],
						}
					: { oldLine: 1, newLine: 1, snippet: [row.text] };
				continue;
			}
			if (!cursor) cursor = { oldLine: 1, newLine: 1, snippet: [] };
			if (cursor.snippet.length < HOVER_SNIPPET_LINES + 2) {
				cursor.snippet.push(`${row.type === 'add' ? '+' : row.type === 'del' ? '-' : ' '}${row.text}`);
			}
			const safeNew = Math.max(1, cursor.newLine);
			if (row.type === 'context') {
				cursor.oldLine += 1;
				cursor.newLine += 1;
			} else if (row.type === 'del') {
				const hover = cursor.snippet.slice(-HOVER_SNIPPET_LINES).join('\n');
				const existing = marksByLine.get(safeNew);
				if (!existing) {
					marksByLine.set(safeNew, { line: safeNew, kind: 'delete', hover });
				}
				cursor.oldLine += 1;
			} else {
				const hover = cursor.snippet.slice(-HOVER_SNIPPET_LINES).join('\n');
				const anchor = Math.max(1, cursor.newLine);
				const prev = marksByLine.get(anchor);
				if (prev && prev.kind === 'delete') {
					prev.kind = 'change';
					prev.hover = `${prev.hover}\n${hover}`;
				} else if (!prev) {
					marksByLine.set(anchor, { line: anchor, kind: 'add', hover });
				}
				cursor.newLine += 1;
			}
			if (marksByLine.size > MAX_MARKS) {
				overflow = true;
				break;
			}
		}
		if (overflow) break;
	}
	const marks = [...marksByLine.values()].sort((a, b) => a.line - b.line);
	return { marks, changeLines: marks.map((m) => m.line), overflow };
}
