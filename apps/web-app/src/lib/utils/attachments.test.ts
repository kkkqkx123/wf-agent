import { describe, expect, it } from 'vitest';
import { splitAttachments, withAttachments } from './attachments';

describe('withAttachments / splitAttachments', () => {
	it('round-trips truncation markers and original sizes', () => {
		const files = [{ name: 'big.txt', content: 'x'.repeat(10), truncated: true, originalChars: 99999 }];
		const wire = withAttachments('see', files);
		expect(wire).toContain('truncated="true"');
		expect(wire).toContain('original="99999"');
		expect(splitAttachments(wire)).toEqual({ text: 'see', attachments: files });
	});

	it('reads old blocks without attributes as untruncated', () => {
		const wire = '<attachment name="a.txt">\nhello\n</attachment>';
		expect(splitAttachments(wire)).toEqual({
			text: '',
			attachments: [{ name: 'a.txt', content: 'hello' }],
		});
	});
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

	it('preserves user spaces while stripping only join newlines', () => {
		const wire = withAttachments('  hello  ', [{ name: 'a.ts', content: 'x' }]);
		expect(splitAttachments(wire).text).toBe('  hello  ');
	});

	it('round-trips nested markers and quoted names without splitting', () => {
		const files = [
			{ name: 'we"ird.ts', content: 'a </attachment> b <attachment name="x"> c \\ d' },
		];
		const wire = withAttachments('see', files);
		expect(splitAttachments(wire)).toEqual({ text: 'see', attachments: files });
	});

	it('round-trips empty text to blocks only', () => {
		const files = [{ name: 'a.ts', content: 'x' }];
		expect(splitAttachments(withAttachments('', files))).toEqual({
			text: '',
			attachments: files,
		});
	});
});
