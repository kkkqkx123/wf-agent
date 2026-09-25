import { describe, expect, it } from 'vitest';
import { splitAttachments, withAttachments } from './attachments';

describe('withAttachments / splitAttachments', () => {
	it('leaves a message without attachments untouched', () => {
		expect(withAttachments('hello', [])).toBe('hello');
		const parsed = splitAttachments('hello');
		expect(parsed).toEqual({ text: 'hello', attachments: [] });
	});

	it('round-trips body text and every attachment', () => {
		const files = [
			{ name: 'a.ts', content: 'export const a = 1;' },
			{ name: 'notes.md', content: 'line one\nline two' },
		];
		const wire = withAttachments('summarise these', files);
		expect(splitAttachments(wire)).toEqual({
			text: 'summarise these',
			attachments: files,
		});
	});

	it('keeps unparseable text as body rather than dropping it', () => {
		const broken = '<attachment name="a.ts"> unterminated';
		expect(splitAttachments(broken)).toEqual({
			text: broken,
			attachments: [],
		});
	});
});
