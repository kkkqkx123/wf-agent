import { describe, expect, it } from 'vitest';
import {
	DEFAULT_NODE_TYPE,
	isBuiltinNodeType,
	nodePorts,
	normalizeNodeType,
	parseStaticNodeType,
	STATIC_NODE_TYPES,
} from './node-kind';

describe('parseStaticNodeType', () => {
	it('accepts every backend name regardless of case', () => {
		for (const type of STATIC_NODE_TYPES) {
			expect(parseStaticNodeType(type)).toBe(type);
			expect(parseStaticNodeType(type.toLowerCase())).toBe(type);
		}
	});

	it('rejects names outside the backend enum', () => {
		expect(parseStaticNodeType('STEP')).toBeNull();
		expect(parseStaticNodeType('CUSTOM')).toBeNull();
		expect(parseStaticNodeType('trigger')).toBeNull();
		expect(parseStaticNodeType('')).toBeNull();
	});

	it('ships a default that the backend accepts', () => {
		expect(parseStaticNodeType(DEFAULT_NODE_TYPE)).toBe(DEFAULT_NODE_TYPE);
	});
});

describe('normalizeNodeType', () => {
	it('canonicalises builtin names', () => {
		expect(normalizeNodeType('llm')).toBe('LLM');
		expect(normalizeNodeType('  route ')).toBe('ROUTE');
	});

	it('keeps plugin-contributed names verbatim', () => {
		expect(normalizeNodeType('Acme.Step')).toBe('Acme.Step');
		expect(normalizeNodeType('CUSTOM')).toBe('CUSTOM');
	});

	it('rejects only blank names', () => {
		expect(normalizeNodeType('')).toBeNull();
		expect(normalizeNodeType('   ')).toBeNull();
	});
});

describe('isBuiltinNodeType', () => {
	it('recognises every backend name', () => {
		for (const type of STATIC_NODE_TYPES) {
			expect(isBuiltinNodeType(type)).toBe(true);
		}
	});

	it('reports plugin types and blanks as non-builtin', () => {
		expect(isBuiltinNodeType('Acme.Step')).toBe(false);
		expect(isBuiltinNodeType('')).toBe(false);
	});
});

describe('nodePorts', () => {
	it('closes the input side of entry nodes', () => {
		expect(nodePorts('START')).toEqual({
			acceptsInput: false,
			emitsOutput: true,
		});
		expect(nodePorts('start_from_message')).toEqual({
			acceptsInput: false,
			emitsOutput: true,
		});
	});

	it('closes the output side of exit nodes', () => {
		expect(nodePorts('END')).toEqual({
			acceptsInput: true,
			emitsOutput: false,
		});
		expect(nodePorts('CONTINUE_FROM_MESSAGE')).toEqual({
			acceptsInput: true,
			emitsOutput: false,
		});
	});

	it('opens both sides of every other node kind', () => {
		for (const type of STATIC_NODE_TYPES) {
			const ports = nodePorts(type);
			if (type === 'START' || type === 'START_FROM_MESSAGE') continue;
			if (type === 'END' || type === 'CONTINUE_FROM_MESSAGE') continue;
			expect(ports).toEqual({ acceptsInput: true, emitsOutput: true });
		}
	});

	it('treats unrecognised kinds as ordinary nodes', () => {
		expect(nodePorts('plugin-step')).toEqual({
			acceptsInput: true,
			emitsOutput: true,
		});
	});
});
