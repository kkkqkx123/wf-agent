export interface TextRun {
	text: string;
	/** Set when this run is a `@kind:value` reference. */
	kind: string | null;
}

const MENTION_PATTERN = /@([a-z][a-z0-9_]*):([\w.-]+)/g;

/** The value half of a reference keeps to characters the parser accepts. */
export function mentionRef(kind: string, label: string): string {
	const value = label
		.trim()
		.toLowerCase()
		.replace(/[^\w.-]+/g, '-')
		.replace(/^-+|-+$/g, '');
	return value ? `@${kind}:${value}` : `@${kind}`;
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
		runs.push({ text: match[0], kind: match[1] });
		cursor = start + match[0].length;
	}
	if (cursor < text.length || runs.length === 0) {
		runs.push({ text: text.slice(cursor), kind: null });
	}
	return runs;
}
