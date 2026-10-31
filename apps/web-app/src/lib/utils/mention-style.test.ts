import { describe, expect, it } from 'vitest';
import { mentionClass, mentionIcon, mentionTone } from './mention-style';

describe('mentionTone', () => {
	it('maps every kind to a stable tone', () => {
		expect(mentionTone('file', 'src/a.ts')).toBe('file');
		expect(mentionTone('file', 'src/')).toBe('directory');
		expect(mentionTone('wf', 'wf-1')).toBe('workflow');
		expect(mentionTone('agent_loop', 'a1')).toBe('agent');
		expect(mentionTone('tool', 'bash')).toBe('tool');
		expect(mentionTone('script', 'build')).toBe('script');
		expect(mentionTone('resource', 'file:///tmp')).toBe('resource');
		expect(mentionTone('unknown', 'x')).toBe('neutral');
	});
});

describe('mentionIcon', () => {
	it('picks the semantic icon per tone', () => {
		expect(mentionIcon('file', 'src/a.ts')).toBe('file');
		expect(mentionIcon('file', 'src/')).toBe('folder');
		expect(mentionIcon('skill', 'pdf')).toBe('sparkles');
		expect(mentionIcon('workflow', 'wf-1')).toBe('workflow');
		expect(mentionIcon('agent', 'a1')).toBe('user');
		expect(mentionIcon('tool', 'bash')).toBe('terminal');
		expect(mentionIcon('script', 'build')).toBe('code');
		expect(mentionIcon('resource', 'file:///tmp')).toBe('link');
		expect(mentionIcon('unknown', 'x')).toBe('hash');
	});
});

describe('mentionClass', () => {
	it('uses semantic tokens and a directory-specific tone', () => {
		expect(mentionClass('file', 'src/a.ts')).toContain('text-info');
		expect(mentionClass('file', 'src/')).toContain('text-success');
		expect(mentionClass('skill', 'pdf')).toContain('text-primary');
		expect(mentionClass('resource', 'file:///tmp')).toContain('text-brand');
		expect(mentionClass('unknown', 'x')).toContain('text-muted-foreground');
	});
});
