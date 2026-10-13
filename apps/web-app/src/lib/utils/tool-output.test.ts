import { describe, expect, it } from 'vitest';
import {
	collapseShellTail,
	collapseText,
	countSummary,
	describeToolGroup,
	diffLineTone,
	groupToolCalls,
	groupToolNames,
	hidesBody,
	inferDisplayKind,
	limitBodyLines,
	parseDisplayPayload,
	previewFor,
	previewSource,
} from '$lib/utils/tool-output';
import type { ToolCallEntry } from '$lib/types/models';

function toolCall(
	overrides: Partial<ToolCallEntry> & { id: string; name: string },
): ToolCallEntry {
	return {
		kind: '',
		status: 'completed',
		startedAt: '',
		durationMs: 0,
		input: '',
		output: '',
		...overrides,
	};
}

describe('collapseText', () => {
	it('passes short text through verbatim', () => {
		const collapsed = collapseText('one\ntwo', 10, 320);
		expect(collapsed).toEqual({
			text: 'one\ntwo',
			overflow: false,
			hiddenLines: 0,
		});
	});

	it('reports no overflow for empty text', () => {
		const collapsed = collapseText('', 10, 320);
		expect(collapsed.overflow).toBe(false);
	});

	it('marks the last visible line and counts hidden lines', () => {
		const text = Array.from({ length: 25 }, (_, i) => `line ${i}`).join('\n');
		const collapsed = collapseText(text, 10, 10_000);
		expect(collapsed.overflow).toBe(true);
		expect(collapsed.hiddenLines).toBe(15);
		expect(collapsed.text.split('\n')[0]).toContain('line 0');
		expect(collapsed.text).toContain('…');
	});

	it('cuts wide text without splitting glyphs', () => {
		const collapsed = collapseText('你好世界hello', 10, 4);
		expect(collapsed.overflow).toBe(true);
		expect(collapsed.hiddenLines).toBe(0);
		expect(collapsed.text).toContain('…');
	});
});

describe('collapseShellTail', () => {
	it('keeps recent lines with a head marker', () => {
		const text = Array.from({ length: 30 }, (_, i) => `out ${i}`).join('\n');
		const collapsed = collapseShellTail(text, 10, 10_000);
		expect(collapsed.overflow).toBe(true);
		expect(collapsed.hiddenLines).toBe(20);
		expect(collapsed.text).toContain('(20 earlier lines)');
		expect(collapsed.text).toContain('out 29');
		expect(collapsed.text).not.toContain('out 0\n');
	});

	it('passes short output through verbatim', () => {
		expect(collapseShellTail('a\nb', 10, 320).text).toBe('a\nb');
	});
});

describe('limitBodyLines', () => {
	it('passes short bodies through uncapped', () => {
		const limited = limitBodyLines('a\nb', 300);
		expect(limited).toEqual({ text: 'a\nb', capped: false, hiddenLines: 0 });
	});

	it('caps long bodies with an omission marker', () => {
		const text = Array.from({ length: 10 }, (_, i) => `row ${i}`).join('\n');
		const limited = limitBodyLines(text, 4);
		expect(limited.capped).toBe(true);
		expect(limited.hiddenLines).toBe(6);
		expect(limited.text).toContain('row 3');
		expect(limited.text).toContain('6 more lines');
		expect(limited.text).not.toContain('row 4\n');
	});
});

describe('inferDisplayKind', () => {
	it('covers shell, edit and exploration tools', () => {
		expect(inferDisplayKind('execute_command')).toBe('shell_tail');
		expect(inferDisplayKind('edit_file')).toBe('diff');
		expect(inferDisplayKind('read_file')).toBe('count');
		expect(inferDisplayKind('grep_search')).toBe('count');
		expect(inferDisplayKind('mystery_tool')).toBe('text');
	});
});

describe('parseDisplayPayload', () => {
	it('reads the wire shape with snake_case keys', () => {
		expect(
			parseDisplayPayload({
				kind: 'shell_tail',
				preview: 'tail',
				truncated: true,
				hidden_lines: 30,
			}),
		).toEqual({
			kind: 'shell_tail',
			preview: 'tail',
			truncated: true,
			hiddenLines: 30,
		});
	});

	it('rejects unknown kinds and shapes', () => {
		expect(parseDisplayPayload({ kind: 'mystery' })).toBeNull();
		expect(parseDisplayPayload(null)).toBeNull();
		expect(parseDisplayPayload('text')).toBeNull();
	});
});

describe('previewSource', () => {
	it('prefers the error on failure', () => {
		expect(
			previewSource({ output: 'full', error: 'denied', success: false }),
		).toBe('denied');
	});

	it('prefers the display preview otherwise', () => {
		expect(
			previewSource({
				output: 'full',
				success: true,
				display: {
					kind: 'text',
					preview: 'head',
					truncated: true,
					hiddenLines: 5,
				},
			}),
		).toBe('head');
	});

	it('falls back to the output', () => {
		expect(previewSource({ output: 'full', success: true })).toBe('full');
	});
});

describe('previewFor', () => {
	it('collapses failures to the short error budget', () => {
		const collapsed = previewFor('execute_command', {
			output: 'traceback\nline1\nline2\nline3\nline4\nline5',
			error: 'denied\nline1\nline2\nline3\nline4\nline5',
			success: false,
		});
		expect(collapsed.overflow).toBe(true);
		expect(collapsed.text.split('\n').length).toBeLessThanOrEqual(5);
	});

	it('keeps shell tails', () => {
		const text = Array.from({ length: 30 }, (_, i) => `out ${i}`).join('\n');
		const collapsed = previewFor('execute_command', {
			output: text,
			success: true,
		});
		expect(collapsed.text).toContain('out 29');
	});
});

describe('hidesBody', () => {
	it('hides successful retrieval bodies', () => {
		expect(hidesBody('read_file', { output: 'a\nb', success: true })).toBe(
			true,
		);
		expect(hidesBody('read_file', { output: 'a\nb', success: false })).toBe(
			false,
		);
		expect(
			hidesBody('execute_command', { output: 'a\nb', success: true }),
		).toBe(false);
	});
});

describe('countSummary', () => {
	it('counts lines and keeps server previews', () => {
		const body = Array.from({ length: 50 }, (_, i) => `file line ${i}`).join(
			'\n',
		);
		expect(countSummary(body)).toBe('50 lines');
		expect(countSummary('single')).toBe('1 line');
		expect(countSummary('')).toBe('empty');
		expect(countSummary(body, 'custom')).toBe('custom');
	});
});

describe('diffLineTone', () => {
	it('classifies unified diff rows', () => {
		expect(diffLineTone('+added')).toBe('add');
		expect(diffLineTone('-removed')).toBe('remove');
		expect(diffLineTone('@@ -1,2 +3,4 @@')).toBe('hunk');
		expect(diffLineTone(' context')).toBeNull();
	});
});

describe('groupToolCalls', () => {
	it('returns no groups for an empty list', () => {
		expect(groupToolCalls([])).toEqual([]);
	});

	it('merges consecutive successful reads', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'read_file' }),
			toolCall({ id: 'b', name: 'grep_search' }),
			toolCall({ id: 'c', name: 'list_files' }),
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0].kind).toBe('read');
		expect(groups[0].key).toBe('a');
		expect(groups[0].members.map((member) => member.id)).toEqual([
			'a',
			'b',
			'c',
		]);
	});

	it('merges consecutive successful edits', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'edit_file' }),
			toolCall({ id: 'b', name: 'apply_patch' }),
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0].kind).toBe('edit');
	});

	it('splits read and edit runs into separate groups', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'read_file' }),
			toolCall({ id: 'b', name: 'edit_file' }),
		]);
		expect(groups.map((group) => group.kind)).toEqual(['read', 'edit']);
	});

	it('keeps shell calls as singles between reads', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'read_file' }),
			toolCall({ id: 'b', name: 'execute_command' }),
			toolCall({ id: 'c', name: 'read_file' }),
		]);
		expect(groups.map((group) => group.kind)).toEqual([
			'read',
			'single',
			'read',
		]);
	});

	it('keeps failures as visible singles', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'read_file' }),
			toolCall({ id: 'b', name: 'read_file', status: 'failed' }),
			toolCall({ id: 'c', name: 'read_file' }),
		]);
		expect(groups.map((group) => group.kind)).toEqual([
			'read',
			'single',
			'read',
		]);
		expect(groups[1].members[0].status).toBe('failed');
	});

	it('keeps unknown kinds as singles', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'mystery_tool' }),
			toolCall({ id: 'b', name: 'mystery_tool' }),
		]);
		expect(groups.map((group) => group.kind)).toEqual(['single', 'single']);
	});

	it('prefers the server display kind over name inference', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'mystery_tool', displayKind: 'count' }),
			toolCall({ id: 'b', name: 'mystery_tool', displayKind: 'count' }),
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0].kind).toBe('read');
	});
});

describe('describeToolGroup', () => {
	it('summarizes groups with counts and singles with names', () => {
		const groups = groupToolCalls([
			toolCall({ id: 'a', name: 'read_file' }),
			toolCall({ id: 'b', name: 'grep_search' }),
			toolCall({ id: 'c', name: 'execute_command' }),
		]);
		expect(describeToolGroup(groups[0])).toBe('2 reads');
		expect(groupToolNames(groups[0])).toBe('read_file, grep_search');
		expect(describeToolGroup(groups[1])).toBe('execute_command');
	});

	it('uses the singular for one-member groups', () => {
		const groups = groupToolCalls([toolCall({ id: 'a', name: 'edit_file' })]);
		expect(describeToolGroup(groups[0])).toBe('1 edit');
	});
});
