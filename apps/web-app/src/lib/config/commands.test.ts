import { describe, expect, it } from 'vitest';
import { matchCommands, parseCommand } from './commands';

describe('parseCommand', () => {
	it('reads the argument of a parameterized command', () => {
		expect(parseCommand('/continue   focus on the parser')).toEqual({
			command: expect.objectContaining({ name: 'continue' }),
			arg: 'focus on the parser',
		});
	});

	it('accepts a command with no argument', () => {
		expect(parseCommand('/new')?.arg).toBe('');
	});

	it('treats an unknown command as ordinary text', () => {
		expect(parseCommand('/deploy production')).toBeNull();
	});

	it('ignores text that is not a command', () => {
		expect(parseCommand('ship /new today')).toBeNull();
	});
});

describe('matchCommands', () => {
	it('filters by the typed prefix', () => {
		expect(matchCommands('re').map((command) => command.name)).toEqual([
			'retry',
			'recap',
			'resume',
		]);
	});

	it('parses the recap command', () => {
		expect(parseCommand('/recap')?.command.name).toBe('recap');
	});

	it('parses the resume command with an optional filter', () => {
		expect(parseCommand('/resume')?.command.name).toBe('resume');
		expect(parseCommand('/resume migration guide')).toEqual({
			command: expect.objectContaining({ name: 'resume' }),
			arg: 'migration guide',
		});
	});
});
