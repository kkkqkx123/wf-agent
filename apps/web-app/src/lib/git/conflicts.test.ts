import { describe, expect, it } from 'vitest';
import { parseConflictBlocks } from './conflicts';

describe('parseConflictBlocks', () => {
	it('parses a normal block with both sides', () => {
		const blocks = parseConflictBlocks([
			'head',
			'<<<<<<< HEAD',
			'ours-a',
			'=======',
			'theirs-a',
			'>>>>>>> branch',
			'tail',
		]);
		expect(blocks.length).toBe(1);
		expect(blocks[0].oursLines).toEqual(['ours-a']);
		expect(blocks[0].theirsLines).toEqual(['theirs-a']);
		expect(blocks[0].startLine).toBe(2);
		expect(blocks[0].endLine).toBe(6);
	});

	it('parses multiple blocks in order', () => {
		const blocks = parseConflictBlocks([
			'<<<<<<< HEAD',
			'ours-a',
			'=======',
			'theirs-a',
			'>>>>>>> b',
			'middle',
			'<<<<<<< HEAD',
			'ours-b',
			'=======',
			'theirs-b',
			'>>>>>>> b',
		]);
		expect(blocks.length).toBe(2);
		expect(blocks[0].index).toBe(0);
		expect(blocks[1].index).toBe(1);
		expect(blocks[1].oursLines).toEqual(['ours-b']);
		expect(blocks[1].theirsLines).toEqual(['theirs-b']);
	});

	it('accepts diff row objects and skips the base separator', () => {
		const blocks = parseConflictBlocks([
			{ text: '<<<<<<< ours' },
			{ text: 'keep-ours' },
			{ text: '||||||| base' },
			{ text: 'base-line' },
			{ text: '=======' },
			{ text: 'keep-theirs' },
			{ text: '>>>>>>> theirs' },
		]);
		expect(blocks.length).toBe(1);
		expect(blocks[0].oursLines).toEqual(['keep-ours']);
		expect(blocks[0].theirsLines).toEqual(['keep-theirs']);
	});

	it('returns no blocks when markers are missing', () => {
		expect(parseConflictBlocks(['plain', 'context'])).toEqual([]);
		expect(parseConflictBlocks(['<<<<<<< HEAD', 'ours-only', 'tail without markers'])).toEqual([]);
	});
});
