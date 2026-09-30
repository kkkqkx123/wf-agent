import { describe, expect, it } from 'vitest';
import {
	describeConnectRejection,
	isConnectValid,
	type ConnectCheck,
} from './canvas-connect';
import type { DisplayEdge } from './display-model';

function check(extra: Partial<ConnectCheck> = {}): ConnectCheck {
	return {
		editMode: true,
		source: 'a',
		target: 'b',
		edges: [],
		hiddenIds: new Set(),
		...extra,
	};
}

function edge(id: string, source: string, target: string): DisplayEdge {
	return { id, source, target };
}

describe('describeConnectRejection', () => {
	it('requires edit mode and distinct endpoints', () => {
		expect(describeConnectRejection(check({ editMode: false }))).toBe(
			'Read-only canvas. Enter edit mode to connect.',
		);
		expect(describeConnectRejection(check({ target: '' }))).toBe(
			'Choose another node to connect.',
		);
		expect(describeConnectRejection(check({ target: 'a' }))).toBe(
			'Cannot self-connect.',
		);
	});

	it('rejects duplicate edges', () => {
		expect(
			describeConnectRejection(check({ edges: [edge('e1', 'a', 'b')] })),
		).toBe('Edge already exists.');
	});

	it('rejects hidden members and group endpoints', () => {
		expect(describeConnectRejection(check({ hiddenIds: new Set(['b']) }))).toBe(
			'Hidden group members cannot connect. Expand the group first.',
		);
		expect(describeConnectRejection(check({ target: 'groupbox:notes' }))).toBe(
			'Groups cannot connect directly. Expand the group first.',
		);
	});

	it('allows plain flow links and mirrors isConnectValid', () => {
		expect(describeConnectRejection(check())).toBeNull();
		expect(isConnectValid(check())).toBe(true);
	});

	it('rejects edges that break the backend boundary rules', () => {
		expect(
			describeConnectRejection(
				check({ sourceKind: 'LLM', targetKind: 'START' }),
			),
		).toBe('START node cannot have incoming edges');
		expect(
			describeConnectRejection(
				check({ sourceKind: 'END', targetKind: 'SCRIPT' }),
			),
		).toBe('END node cannot have outgoing edges');
		expect(
			describeConnectRejection(
				check({ sourceKind: 'llm', targetKind: 'start_from_message' }),
			),
		).toBe('START_FROM_MESSAGE node cannot have incoming edges');
		expect(
			describeConnectRejection(
				check({ sourceKind: 'CONTINUE_FROM_MESSAGE', targetKind: 'SCRIPT' }),
			),
		).toBe('CONTINUE_FROM_MESSAGE node cannot have outgoing edges');
	});

	it('allows edges between ordinary node kinds', () => {
		expect(
			isConnectValid(check({ sourceKind: 'LLM', targetKind: 'SCRIPT' })),
		).toBe(true);
		expect(
			isConnectValid(check({ sourceKind: 'SCRIPT', targetKind: 'END' })),
		).toBe(true);
		expect(
			isConnectValid(check({ sourceKind: 'START', targetKind: 'LLM' })),
		).toBe(true);
	});

	it('keeps topology checks ahead of port checks', () => {
		expect(
			describeConnectRejection(
				check({
					edges: [edge('e1', 'a', 'b')],
					sourceKind: 'LLM',
					targetKind: 'START',
				}),
			),
		).toBe('Edge already exists.');
	});
});
