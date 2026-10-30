import { describe, expect, it } from 'vitest';
import { truncateTitle } from './title';

describe('truncateTitle', () => {
	it('keeps a short prompt as is', () => {
		expect(truncateTitle('deploy the api')).toBe('deploy the api');
	});

	it('keeps only the first line', () => {
		expect(truncateTitle('first line\nsecond line')).toBe('first line');
	});

	it('caps long prompts at sixty characters', () => {
		const title = truncateTitle('a'.repeat(100));
		expect(title).toBe('a'.repeat(60));
	});

	it('yields empty for blank input', () => {
		expect(truncateTitle('   ')).toBe('');
		expect(truncateTitle('')).toBe('');
	});
});
