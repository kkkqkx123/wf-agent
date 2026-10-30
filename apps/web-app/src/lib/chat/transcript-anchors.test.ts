import { describe, expect, it } from 'vitest';
import {
	isTranscriptAnchor,
	nextAnchorIndex,
	prevAnchorIndex,
	resolveTranscriptAnchorKind,
	TRANSCRIPT_ANCHOR_KINDS,
} from './transcript-anchors';
import type { LoopMessage } from '#lib/types/models';

function message(overrides: Partial<LoopMessage>): LoopMessage {
	return {
		id: 'id',
		role: 'assistant',
		content: 'text',
		createdAt: '',
		toolName: null,
		...overrides,
	};
}

describe('resolveTranscriptAnchorKind', () => {
	it('treats user messages as prompts', () => {
		expect(resolveTranscriptAnchorKind(message({ role: 'user' }))).toBe('prompt');
	});

	it('treats todo tool messages as todo anchors', () => {
		expect(
			resolveTranscriptAnchorKind(message({ role: 'tool', toolName: 'update_todo_list' })),
		).toBe('todo');
	});

	it('treats tool messages as tool anchors', () => {
		expect(
			resolveTranscriptAnchorKind(message({ role: 'tool', toolName: 'shell.exec' })),
		).toBe('tool');
	});

	it('ignores ordinary assistant messages', () => {
		expect(resolveTranscriptAnchorKind(message({}))).toBeNull();
	});
});

describe('isTranscriptAnchor', () => {
	it('enables prompts, todos and tools by default', () => {
		expect(TRANSCRIPT_ANCHOR_KINDS).toEqual(['prompt', 'todo', 'tool']);
		expect(isTranscriptAnchor(message({ role: 'user' }))).toBe(true);
		expect(isTranscriptAnchor(message({ toolName: 'update_todo_list' }))).toBe(true);
		expect(
			isTranscriptAnchor(message({ role: 'tool', toolName: 'shell.exec' })),
		).toBe(true);
		expect(isTranscriptAnchor(message({}))).toBe(false);
	});

	it('honours an explicit kind selection', () => {
		expect(isTranscriptAnchor(message({ role: 'user' }), ['todo'])).toBe(false);
		expect(isTranscriptAnchor(message({ toolName: 'update_todo_list' }), ['todo'])).toBe(true);
		expect(isTranscriptAnchor(message({ role: 'user' }), [])).toBe(false);
	});
});

describe('nextAnchorIndex', () => {
	it('returns null without anchors', () => {
		expect(nextAnchorIndex([], 0)).toBeNull();
	});

	it('moves strictly forward from between anchors', () => {
		expect(nextAnchorIndex([2, 5, 9], 3)).toBe(5);
	});

	it('skips the anchor under the viewport', () => {
		expect(nextAnchorIndex([2, 5, 9], 5)).toBe(9);
	});

	it('returns null past the last anchor', () => {
		expect(nextAnchorIndex([2, 5], 9)).toBeNull();
	});
});

describe('prevAnchorIndex', () => {
	it('returns null without anchors', () => {
		expect(prevAnchorIndex([], 4)).toBeNull();
	});

	it('moves strictly backward from between anchors', () => {
		expect(prevAnchorIndex([2, 5, 9], 6)).toBe(5);
	});

	it('skips the anchor under the viewport', () => {
		expect(prevAnchorIndex([2, 5, 9], 5)).toBe(2);
	});

	it('returns null at or before the first anchor', () => {
		expect(prevAnchorIndex([2, 5], 2)).toBeNull();
		expect(prevAnchorIndex([2, 5], 0)).toBeNull();
	});
});
