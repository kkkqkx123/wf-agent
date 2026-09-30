/**
 * Decide whether a trace payload reads better as Markdown than as JSON.
 * Returns the source text when the value is a Markdown-like string,
 * otherwise null so the caller falls back to the JSON viewer.
 */
export function extractMarkdownText(value: unknown): string | null {
	if (typeof value !== 'string') return null;
	if (value.trim() === '') return null;
	const trimmed = value.trimStart();
	if (trimmed.startsWith('{') || trimmed.startsWith('[')) return null;
	if (!value.includes('\n') && !hasMarkdownMarker(value)) return null;
	return value;
}

const MARKDOWN_MARKERS: RegExp[] = [
	/^#{1,6}\s/m,
	/\*\*.+\*\*/,
	/`[^`]+`/,
	/^\s*[-*+]\s+\S/m,
	/^\s*\d+[.)]\s+\S/m,
	/^\s*>\s+\S/m,
	/\[[^\]]+\]\([^)]+\)/,
];

function hasMarkdownMarker(text: string): boolean {
	return MARKDOWN_MARKERS.some((pattern) => pattern.test(text));
}
