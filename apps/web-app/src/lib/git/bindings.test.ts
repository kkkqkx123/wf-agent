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

	it('derives controls from non-printable priority then visible bindings', () => {
		const controls = visibleControls('files', 6);
		expect(controls.length).toBeLessThanOrEqual(6);
		expect(controls.length).toBeGreaterThan(0);
		const nonPrintable = controls.filter((c) => c.nonPrintable);
		expect(nonPrintable.length).toBeGreaterThan(0);
		expect(nonPrintable.every((c) => c.controlPriority > 0)).toBe(true);
		const printable = controls.filter((c) => !c.nonPrintable);
		expect(printable.every((c) => !c.hidden)).toBe(true);
		expect(controls[0]?.nonPrintable).toBe(true);
	});

	it('marks space and enter bindings as non-printable controls', () => {
		const toggle = findBinding('stage.toggle');
		expect(toggle?.nonPrintable).toBe(true);
		expect(toggle?.legacyKey).toBe('<space>');
		const enter = findBinding('submodule.enter');
		expect(enter?.nonPrintable).toBe(true);
		expect(enter?.legacyKey).toBe('<enter>');
		const emptyAlias = findBinding('stash.rename');
		expect(emptyAlias?.nonPrintable).toBe(true);
		expect(emptyAlias?.controlPriority).toBeGreaterThan(0);
	});

	it('keeps printable bindings without control metadata', () => {
		expect(findBinding('refresh')?.nonPrintable).toBe(false);
		expect(findBinding('refresh')?.legacyKey).toBeNull();
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
