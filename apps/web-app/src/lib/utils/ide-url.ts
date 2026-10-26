import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

/** IDE deep-link helper shared by mention jumps, checkpoint jumps and file search. */

export interface IdeLocation {
	path: string;
	line: number | null;
	col: number | null;
}

function toPositiveInt(value: unknown): number | null {
	const n = typeof value === 'string' ? Number.parseInt(value, 10) : Number(value);
	if (!Number.isFinite(n) || n < 1) return null;
	return Math.floor(n);
}

/** Build an IDE address with optional 1-based line/column. */
export function buildIdeUrl(path: string, line?: number | null, col?: number | null): string {
	const clean = path.replace(/^\.?\/+/, '');
	const params = new URLSearchParams({ path: clean });
	const lineNo = toPositiveInt(line ?? null);
	const colNo = toPositiveInt(col ?? null);
	if (lineNo !== null) params.set('line', String(lineNo));
	if (colNo !== null) params.set('col', String(colNo));
	return `${resolve('/ide')}?${params.toString()}`;
}

/** Parse IDE query parameters. Row/column are clamped to >= 1 when present. */
export function parseIdeQuery(search: Pick<URLSearchParams, 'get'>): IdeLocation | null {
	const path = (search.get('path') ?? '').trim().replace(/^\.?\/+/, '');
	if (!path) return null;
	return {
		path,
		line: search.get('line') === null ? null : (toPositiveInt(search.get('line')) ?? null),
		col: search.get('col') === null ? null : (toPositiveInt(search.get('col')) ?? null),
	};
}

/** Clamp a 1-based line/column against a loaded file. */
export function clampIdeLocation(
	line: number | null,
	col: number | null,
	lineCount: number,
	colCountForLine?: (line: number) => number,
): { line: number | null; col: number | null } {
	if (line === null) return { line: null, col: null };
	const clampedLine = Math.min(Math.max(1, line), Math.max(1, lineCount));
	if (col === null) return { line: clampedLine, col: null };
	const maxCol = colCountForLine ? Math.max(1, colCountForLine(clampedLine) + 1) : col;
	return { line: clampedLine, col: Math.min(Math.max(1, col), maxCol) };
}

/** Best-effort navigation to an IDE location. Never throws to callers. */
export async function gotoIde(
	path: string,
	line?: number | null,
	col?: number | null,
): Promise<void> {
	try {
		await goto(buildIdeUrl(path, line, col));
	} catch {
		// Navigation is best-effort; the IDE shows the tab error state.
	}
}
