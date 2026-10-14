/**
 * Collapsed tool-output rendering shared by the live chat, the history
 * bubbles and the audit views.
 *
 * These are pure width-independent text transforms mirroring the TUI
 * `tool_output` helper: truncation only shapes the displayed rows, while the
 * full text stays available for on-demand viewing. Counts are
 * grapheme-aware so CJK and wide glyphs are never split.
 *
 * Tool-name matching here is a frontend-local fallback for persisted
 * history, which never carries a display payload. The live path prefers the
 * protocol display payload when the server provides one.
 */

import {
	TOOL_BODY_MAX_LINES,
	TOOL_PREVIEW_MAX_CHARS,
	TOOL_PREVIEW_MAX_LINES,
	TOOL_SHELL_PREVIEW_LINES,
	resolveToolOutputBudgets,
} from '$lib/config/tool-output';
import type { ToolOutputBudgets } from '$lib/config/tool-output';
import type { ToolCallEntry } from '$lib/types/models';

/** Presentation kind selected by the tool that produced the result. */
export type DisplayKind = 'shell_tail' | 'diff' | 'count' | 'text';

/** Frontend-facing summary of one tool result, produced alongside the full
 * result. `preview` is already a small excerpt; the full text stays in the
 * sibling result field for on-demand viewing. */
export interface DisplayPayload {
	kind: DisplayKind;
	preview: string;
	truncated: boolean;
	hiddenLines: number;
}

/** Collapsed preview plus overflow accounting for the summary hint. */
export interface Collapsed {
	text: string;
	overflow: boolean;
	hiddenLines: number;
}

/** Expanded inline body plus whether the row cap was hit. */
export interface LimitedBody {
	text: string;
	capped: boolean;
	hiddenLines: number;
}

const DISPLAY_KINDS: DisplayKind[] = ['shell_tail', 'diff', 'count', 'text'];

/** Read a wire display payload (`snake_case`, as the backend serializes it).
 * Returns null for anything else so unknown shapes fall back to inference. */
export function parseDisplayPayload(value: unknown): DisplayPayload | null {
	if (!value || typeof value !== 'object') return null;
	const record = value as Record<string, unknown>;
	const kind = record.kind;
	if (
		typeof kind !== 'string' ||
		!DISPLAY_KINDS.includes(kind as DisplayKind)
	) {
		return null;
	}
	const preview = record.preview;
	const truncated = record.truncated ?? false;
	const hidden = record.hidden_lines ?? record.hiddenLines ?? 0;
	return {
		kind: kind as DisplayKind,
		preview: typeof preview === 'string' ? preview : '',
		truncated: truncated === true,
		hiddenLines:
			typeof hidden === 'number' && Number.isFinite(hidden)
				? Math.max(0, Math.trunc(hidden))
				: 0,
	};
}

/** Command-output tool names, mirroring the engine fold routing shape. */
const SHELL_TOOL_NAMES = new Set([
	'execute_command',
	'backend_shell',
	'shell_output',
	'shell_wait',
	'shell_kill',
	'shell_send_input',
	'shell_resize',
	'get_or_create_shell',
	'execute_in_session',
	'release_sessions_for_task',
	'session_observe',
]);

/** Read and retrieval tool names whose bodies stay hidden by default. */
const EXPLORATION_TOOL_NAMES = new Set([
	'read_file',
	'read_file_folded',
	'list_files',
	'glob_search',
	'grep_search',
	'code_search',
	'code_keyword_search',
	'retrieve_code',
	'web_fetch',
	'web_search',
]);

const EDIT_TOOL_NAMES = new Set([
	'write_file',
	'edit_file',
	'apply_diff',
	'apply_patch',
]);

/** Infer the presentation kind from a tool name. Used when no protocol
 * display payload is available. */
export function inferDisplayKind(toolName: string): DisplayKind {
	if (SHELL_TOOL_NAMES.has(toolName)) return 'shell_tail';
	if (EDIT_TOOL_NAMES.has(toolName)) return 'diff';
	if (EXPLORATION_TOOL_NAMES.has(toolName)) return 'count';
	return 'text';
}

function splitGraphemes(text: string): string[] {
	try {
		return Array.from(
			new Intl.Segmenter().segment(text),
			(segment) => segment.segment,
		);
	} catch {
		return Array.from(text);
	}
}

function clampChars(text: string, maxChars: number): string {
	const graphemes = splitGraphemes(text);
	if (graphemes.length <= maxChars) return text;
	return `${graphemes.slice(0, Math.max(0, maxChars)).join('')}…`;
}

/** Keep the head of `text` within the line and grapheme budgets. Overlong
 * input marks the last visible line and reports the hidden line count. */
export function collapseText(
	text: string,
	maxLines: number = TOOL_PREVIEW_MAX_LINES,
	maxChars: number = TOOL_PREVIEW_MAX_CHARS,
): Collapsed {
	const lines = text.split('\n');
	const cappedLines = Math.max(1, maxLines);
	if (lines.length <= cappedLines && splitGraphemes(text).length <= maxChars) {
		return { text, overflow: false, hiddenLines: 0 };
	}
	const hiddenLines = Math.max(0, lines.length - cappedLines);
	const visible = lines.slice(0, cappedLines);
	if (hiddenLines > 0) {
		visible[visible.length - 1] += '…';
	}
	return {
		text: clampChars(visible.join('\n'), maxChars),
		overflow: true,
		hiddenLines,
	};
}

/** Keep the tail of `text` within the budgets, heading it with the hidden
 * line count. Command output converges on recent lines, so the head is the
 * side that is dropped. */
export function collapseShellTail(
	text: string,
	maxLines: number = TOOL_SHELL_PREVIEW_LINES,
	maxChars: number = TOOL_PREVIEW_MAX_CHARS,
): Collapsed {
	const lines = text.split('\n');
	const cappedLines = Math.max(1, maxLines);
	if (lines.length <= cappedLines && splitGraphemes(text).length <= maxChars) {
		return { text, overflow: false, hiddenLines: 0 };
	}
	const hiddenLines = Math.max(0, lines.length - cappedLines);
	const tail = lines.slice(lines.length - cappedLines);
	const composed =
		hiddenLines > 0
			? `(${hiddenLines} earlier lines)\n${tail.join('\n')}`
			: tail.join('\n');
	return {
		text: clampChars(composed, maxChars),
		overflow: true,
		hiddenLines,
	};
}

/** Cap one expanded inline body to `maxLines` head lines. Returns the body
 * and whether the cap was hit. */
export function limitBodyLines(
	text: string,
	maxLines: number = TOOL_BODY_MAX_LINES,
): LimitedBody {
	const lines = text.split('\n');
	if (lines.length <= Math.max(1, maxLines)) {
		return { text, capped: false, hiddenLines: 0 };
	}
	const hiddenLines = lines.length - Math.max(1, maxLines);
	return {
		text: `${lines.slice(0, Math.max(1, maxLines)).join('\n')}\n[… ${hiddenLines} more lines]`,
		capped: true,
		hiddenLines,
	};
}

export interface PreviewInput {
	output: string;
	error?: string | null;
	success?: boolean;
	display?: DisplayPayload | null;
}

/** Preview source: the error wins on failure, the display preview or the
 * output otherwise. */
export function previewSource(input: PreviewInput): string {
	if (input.success === false) {
		const error = (input.error ?? '').trim();
		if (error) return input.error as string;
	}
	const preview = (input.display?.preview ?? '').trim();
	if (preview) return input.display?.preview as string;
	return input.output;
}

/** Collapse one tool output to its preview rows. Failures collapse the
 * error text; retrieval bodies are never previewed here (the count summary
 * is their whole inline story). Budgets default to the balanced preset so
 * callers without a preference still render. */
export function previewFor(
	toolName: string,
	input: PreviewInput,
	budgets: ToolOutputBudgets = resolveToolOutputBudgets('balanced'),
): Collapsed {
	const kind = input.display?.kind ?? inferDisplayKind(toolName);
	const source = previewSource(input);
	if (input.success === false && (input.error ?? '').trim()) {
		return collapseText(
			source,
			budgets.errorPreviewLines,
			budgets.previewMaxChars,
		);
	}
	if (kind === 'shell_tail') {
		return collapseShellTail(
			source,
			budgets.shellPreviewLines,
			budgets.previewMaxChars,
		);
	}
	return collapseText(source, budgets.previewMaxLines, budgets.previewMaxChars);
}

/** Whether the body stays hidden by default: retrieval output on success
 * shows only the count summary until the user expands it. */
export function hidesBody(toolName: string, input: PreviewInput): boolean {
	const kind = input.display?.kind ?? inferDisplayKind(toolName);
	return kind === 'count' && input.success !== false;
}

/** One-line count summary for hidden retrieval bodies, e.g. `50 lines`. */
export function countSummary(output: string, preview?: string | null): string {
	if (preview && preview.trim()) return preview.trim();
	const total = output ? output.split('\n').length : 0;
	if (total === 0) return 'empty';
	if (total === 1) return '1 line';
	return `${total} lines`;
}

/** Diff row tone for inline coloring: added, removed, hunk header, or
 * plain context. */
export type DiffTone = 'add' | 'remove' | 'hunk' | null;

export function diffLineTone(line: string): DiffTone {
	if (line.startsWith('@@')) return 'hunk';
	if (line.startsWith('+')) return 'add';
	if (line.startsWith('-')) return 'remove';
	return null;
}

/** Group shape for one row of a tool list. Consecutive successful
 * retrieval calls merge into a `read` group and consecutive successful
 * file edits into an `edit` group; everything else stays `single` so
 * shell output and failures keep their own rows. */
export type ToolGroupKind = 'single' | 'read' | 'edit';

export interface ToolCallGroup {
	/** First member id: stable while a live list only appends. */
	key: string;
	kind: ToolGroupKind;
	members: ToolCallEntry[];
}

/** Whether one entry may merge with its neighbours. Failures never merge
 * so they stay visible; shell and unknown kinds keep their own rows so
 * command tails are never buried inside a group. */
function groupableKind(entry: ToolCallEntry): ToolGroupKind | null {
	if (entry.status === 'failed') return null;
	const kind = entry.displayKind ?? inferDisplayKind(entry.name);
	if (kind === 'count') return 'read';
	if (kind === 'diff') return 'edit';
	return null;
}

/** Fold one ordered tool list into display rows. Grouping is a pure
 * render-time derivation: no state is stored, and keys derive from member
 * ids so a collapsed group survives list appends. */
export function groupToolCalls(entries: ToolCallEntry[]): ToolCallGroup[] {
	const groups: ToolCallGroup[] = [];
	for (const entry of entries) {
		const kind = groupableKind(entry);
		const tail = groups[groups.length - 1];
		if (kind !== null && tail && tail.kind === kind) {
			tail.members.push(entry);
			continue;
		}
		groups.push({ key: entry.id, kind: kind ?? 'single', members: [entry] });
	}
	return groups;
}

/** One-line header for a group row, e.g. `3 reads`. Singles have no header. */
export function describeToolGroup(group: ToolCallGroup): string {
	const count = group.members.length;
	if (group.kind === 'read') return `${count} read${count === 1 ? '' : 's'}`;
	if (group.kind === 'edit') return `${count} edit${count === 1 ? '' : 's'}`;
	return group.members[0]?.name ?? 'tool';
}

/** Distinct tool names inside a group for the header subtitle. */
export function groupToolNames(group: ToolCallGroup): string {
	return [...new Set(group.members.map((member) => member.name))].join(', ');
}
