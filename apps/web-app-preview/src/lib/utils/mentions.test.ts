import { describe, expect, it } from 'vitest';
import { mentionRef, textRuns } from './mentions';

describe('mentionRef', () => {
	it('reduces a label to the characters the parser accepts', () => {
		expect(mentionRef('workflow', 'Nightly Build Flow')).toBe(
			'@workflow:nightly-build-flow',
		);
	});

	it('drops the value half when nothing survives', () => {
		expect(mentionRef('workflow', '  ')).toBe('@workflow');
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

	it('returns the whole text when there is no reference', () => {
		expect(textRuns('plain sentence')).toEqual([
			{ text: 'plain sentence', kind: null },
		]);
	});
});
