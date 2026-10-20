import { describe, expect, it } from 'vitest';
import {
	entityMention,
	fileMentionRef,
	parseMentionValue,
	textRuns,
} from './mentions';

describe('entityMention', () => {
	it('keeps the canonical id unchanged', () => {
		expect(entityMention('execution', 'exec-123')).toBe('@execution:exec-123');
	});

	it('drops the value half when the id is empty', () => {
		expect(entityMention('workflow', '  ')).toBe('@workflow');
	});
});

describe('fileMentionRef', () => {
	it('keeps path separators and strips a leading dot-slash', () => {
		expect(fileMentionRef('./src/lib/a.ts')).toBe('@file:src/lib/a.ts');
	});

	it('appends an inclusive line range', () => {
		expect(fileMentionRef('src/a.ts', 10, 20)).toBe('@file:src/a.ts#10-20');
		expect(fileMentionRef('src/a.ts', 10)).toBe('@file:src/a.ts#10');
	});

	it('degrades an empty path to the bare kind', () => {
		expect(fileMentionRef('  ')).toBe('@file');
	});
});

describe('parseMentionValue', () => {
	it('splits a path from its line segment', () => {
		expect(parseMentionValue('src/a.ts#10-20')).toEqual({
			value: 'src/a.ts',
			lineStart: 10,
			lineEnd: 20,
		});
	});

	it('treats a single line as a one-line range', () => {
		expect(parseMentionValue('src/a.ts#7')).toEqual({
			value: 'src/a.ts',
			lineStart: 7,
			lineEnd: 7,
		});
	});

	it('returns the value unchanged without a segment', () => {
		expect(parseMentionValue('build')).toEqual({
			value: 'build',
			lineStart: null,
			lineEnd: null,
		});
	});
});

describe('textRuns', () => {
	it('marks reference runs and keeps the plain text around them', () => {
		expect(textRuns('compare @workflow:build with @tool:git')).toEqual([
			{ text: 'compare ', kind: null },
			{ text: '@workflow:build', kind: 'workflow' },
			{ text: ' with ', kind: null },
			{ text: '@tool:git', kind: 'tool' },
		]);
	});

	it('keeps a file path and line segment in one run', () => {
		expect(textRuns('open @file:src/a.ts#10-20 now')).toEqual([
			{ text: 'open ', kind: null },
			{ text: '@file:src/a.ts#10-20', kind: 'file' },
			{ text: ' now', kind: null },
		]);
	});

	it('returns the whole text when there is no reference', () => {
		expect(textRuns('plain sentence')).toEqual([
			{ text: 'plain sentence', kind: null },
		]);
	});

	it('classifies file path shorthand as a file mention', () => {
		expect(textRuns('open @src/main.rs#10-20 now')).toEqual([
			{ text: 'open ', kind: null },
			{ text: '@src/main.rs#10-20', kind: 'file' },
			{ text: ' now', kind: null },
		]);
	});
});
