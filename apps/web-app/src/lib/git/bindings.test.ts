import { describe, expect, it } from 'vitest';
import { GIT_BINDINGS, bindingsFor, checkDrift, findBinding, visibleControls } from './bindings';

describe('git bindings', () => {
	it('keeps action ids unique', () => {
		const ids = GIT_BINDINGS.map((b) => b.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it('keeps aliases unique inside each context', () => {
		const contexts = new Set(GIT_BINDINGS.flatMap((b) => b.contexts));
		for (const context of contexts) {
			const scoped = GIT_BINDINGS.filter(
				(b) => b.contexts.includes(context) || b.contexts.includes('global'),
			);
			const aliases = scoped.flatMap((b) => b.aliases);
			const duplicates = aliases.filter((alias, index) => aliases.indexOf(alias) !== index);
			expect(duplicates, `duplicate aliases in ${context}`).toEqual([]);
		}
	});

	it('declares a known danger level for every binding', () => {
		for (const binding of GIT_BINDINGS) {
			expect(['readonly', 'recoverable', 'destructive', 'history_rewrite']).toContain(
				binding.danger,
			);
		}
	});

	it('returns global bindings for every context and sorts by weight', () => {
		const items = bindingsFor('files');
		expect(items.map((b) => b.id)).toContain('refresh');
		const weights = items.map((b) => b.weight);
		expect([...weights].sort((a, b) => b - a)).toEqual(weights);
	});

	it('limits visible controls to non-hidden bindings', () => {
		const controls = visibleControls('files', 6);
		expect(controls.length).toBeLessThanOrEqual(6);
		expect(controls.every((c) => !c.hidden)).toBe(true);
	});

	it('reports drift in both directions', () => {
		const drift = checkDrift(['refresh', 'unknown.action']);
		expect(drift.unknown).toEqual(['unknown.action']);
		expect(drift.missing.length).toBeGreaterThan(0);
		expect(drift.missing).not.toContain('refresh');
	});

	it('finds a binding by id', () => {
		expect(findBinding('push')?.danger).toBe('history_rewrite');
		expect(findBinding('nope')).toBeUndefined();
	});
});
