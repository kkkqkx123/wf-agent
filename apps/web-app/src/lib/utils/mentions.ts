export interface TextRun {
	text: string;
	/** Set when this run is a `@kind:value` reference. */
	kind: string | null;
}

/**
 * Reference syntax `@kind:value`. Entity values keep to word characters and
 * dots/dashes; file values may also carry path separators and an optional
 * `#start-end` line segment, matching the CLI mention grammar.
 */
/** Canonical `@kind:value` references plus file-path shorthand such as
 *  `@src/main.rs` or `@src/main.rs#10-20`, mirroring the TUI parser so the
 *  two surfaces classify the same input consistently. */
const MENTION_PATTERN =
	/@([a-z][a-z0-9_]*):([\w./-]+(?:#\d+(?:-\d+)?)?)|@([\w./-]*[./][\w./-]*(?:#\d+(?:-\d+)?)?)/g;

/** A mention value split into its target and optional 1-based line segment. */
export interface MentionTarget {
	value: string;
	lineStart: number | null;
	lineEnd: number | null;
}

/** Build a canonical entity reference from its backend id. The id is kept
 * verbatim so the reference can be resolved later; only surrounding
 * whitespace is trimmed. */
export function entityMention(kind: string, id: string): string {
	const value = id.trim();
	return value ? `@${kind}:${value}` : `@${kind}`;
}

/**
 * Build a file reference from a path and an optional inclusive line range.
 * Leading `./` and stray `#` are dropped so the reference re-parses cleanly.
 */
export function fileMentionRef(
	path: string,
	lineStart?: number,
	lineEnd?: number,
): string {
	const clean = path
		.trim()
		.replace(/^\.?\/+/, '')
		.replace(/#/g, '');
	if (!clean) return '@file';
	if (lineStart === undefined) return `@file:${clean}`;
	const end = lineEnd ?? lineStart;
	return end === lineStart
		? `@file:${clean}#${lineStart}`
		: `@file:${clean}#${lineStart}-${end}`;
}

/** Split a reference value into its target and line segment. */
export function parseMentionValue(raw: string): MentionTarget {
	const match = /^(.*?)#(\d+)(?:-(\d+))?$/.exec(raw);
	if (!match) return { value: raw, lineStart: null, lineEnd: null };
	const start = Number.parseInt(match[2], 10);
	const end = match[3] ? Number.parseInt(match[3], 10) : start;
	return { value: match[1], lineStart: start, lineEnd: end };
}

/** Split message text so mention markers can be rendered as badges. */
export function textRuns(text: string): TextRun[] {
	const runs: TextRun[] = [];
	let cursor = 0;
	for (const match of text.matchAll(MENTION_PATTERN)) {
		const start = match.index ?? 0;
		if (start > cursor) {
			runs.push({ text: text.slice(cursor, start), kind: null });
		}
		// Canonical mentions carry their kind in group 1; file-path
		// shorthand (group 3) has no kind and is classified as `file`,
		// matching the TUI parser.
		const kind = match[1] ?? 'file';
		runs.push({ text: match[0], kind });
		cursor = start + match[0].length;
	}
	if (cursor < text.length || runs.length === 0) {
		runs.push({ text: text.slice(cursor), kind: null });
	}
	return runs;
}
