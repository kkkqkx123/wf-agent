import { describe, expect, it } from 'vitest';
import { extractMarkdownText } from './markdown';

describe('extractMarkdownText', () => {
	it('returns null for non-string payloads', () => {
		expect(extractMarkdownText(null)).toBeNull();
		expect(extractMarkdownText(undefined)).toBeNull();
		expect(extractMarkdownText(42)).toBeNull();
		expect(extractMarkdownText({ text: '# hi' })).toBeNull();
		expect(extractMarkdownText(['# hi'])).toBeNull();
	});

	it('returns null for blank strings', () => {
		expect(extractMarkdownText('')).toBeNull();
		expect(extractMarkdownText('   ')).toBeNull();
	});

	it('returns null for JSON-looking strings', () => {
		expect(extractMarkdownText('{"answer": 1}')).toBeNull();
		expect(extractMarkdownText('  [1, 2]')).toBeNull();
	});

	it('returns null for plain short strings without markers', () => {
		expect(extractMarkdownText('done')).toBeNull();
		expect(extractMarkdownText('task completed')).toBeNull();
	});

	it('returns multi-line prose as markdown', () => {
		const text = 'First line\nSecond line';
		expect(extractMarkdownText(text)).toBe(text);
	});

	it('returns single-line markdown with markers', () => {
		expect(extractMarkdownText('# Heading')).toBe('# Heading');
		expect(extractMarkdownText('Use `code` here')).toBe('Use `code` here');
		expect(extractMarkdownText('**bold** answer')).toBe('**bold** answer');
		expect(extractMarkdownText('See [docs](https://example.com)')).toBe(
			'See [docs](https://example.com)',
		);
	});

	it('returns list and quote markers', () => {
		expect(extractMarkdownText('- item one')).toBe('- item one');
		expect(extractMarkdownText('1. first step')).toBe('1. first step');
		expect(extractMarkdownText('> quoted')).toBe('> quoted');
	});
});
