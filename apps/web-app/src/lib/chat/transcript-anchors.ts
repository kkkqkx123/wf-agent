import type { LoopMessage } from '#lib/types/models';

/**
 * Transcript anchor kinds shared by every transcript view. Names describe
 * the behaviour behind the message, not the component that renders it.
 * Failure-like kinds stay out until the message model carries an outcome;
 * guessing from content would misfire on ordinary prose.
 */
export type TranscriptAnchorKind = 'prompt' | 'todo';

/** Anchor kinds enabled when the caller passes no explicit selection. */
export const TRANSCRIPT_ANCHOR_KINDS: readonly TranscriptAnchorKind[] = ['prompt', 'todo'];

/** Kind of anchor behind one message, or null when it jumps nowhere. */
export function resolveTranscriptAnchorKind(message: LoopMessage): TranscriptAnchorKind | null {
	if (message.role === 'user') return 'prompt';
	if (message.toolName === 'update_todo_list') return 'todo';
	return null;
}

/**
 * Anchor predicate for transcript scrollers. The enabled list selects which
 * positions count as anchors; callers keep the default until a settings
 * surface wires a real choice through.
 */
export function isTranscriptAnchor(
	message: LoopMessage,
	enabled: readonly TranscriptAnchorKind[] = TRANSCRIPT_ANCHOR_KINDS,
): boolean {
	const kind = resolveTranscriptAnchorKind(message);
	return kind !== null && enabled.includes(kind);
}

/**
 * Index of the nearest anchor strictly after the viewport position.
 * Null means the run has no later anchor and the caller resumes the tail.
 */
export function nextAnchorIndex(
	anchorIndexes: readonly number[],
	currentIndex: number,
): number | null {
	for (const index of anchorIndexes) {
		if (index > currentIndex) return index;
	}
	return null;
}

/**
 * Index of the nearest anchor strictly before the viewport position.
 * Null means nothing precedes the viewport and the caller holds the head.
 */
export function prevAnchorIndex(
	anchorIndexes: readonly number[],
	currentIndex: number,
): number | null {
	let target: number | null = null;
	for (const index of anchorIndexes) {
		if (index >= currentIndex) break;
		target = index;
	}
	return target;
}
