import { describe, expect, it } from 'vitest';
import {
	describeToolOutputBudgets,
	parseToolOutputDetail,
	resolveToolOutputBudgets,
	TOOL_OUTPUT_DETAIL_ORDER,
	TOOL_OUTPUT_PRESETS,
} from '#lib/config/tool-output';
import { previewFor } from '#lib/utils/tool-output';

describe('tool output presets', () => {
	it('covers compact, balanced and verbose', () => {
		expect(TOOL_OUTPUT_DETAIL_ORDER).toEqual([
			'compact',
			'balanced',
			'verbose',
		]);
		for (const detail of TOOL_OUTPUT_DETAIL_ORDER) {
			const budgets = resolveToolOutputBudgets(detail);
			expect(budgets).toBe(TOOL_OUTPUT_PRESETS[detail]);
		}
	});

	it('keeps the error floor on every preset', () => {
		for (const detail of TOOL_OUTPUT_DETAIL_ORDER) {
			expect(
				resolveToolOutputBudgets(detail).errorPreviewLines,
			).toBeGreaterThanOrEqual(2);
		}
	});

	it('orders presets by preview size', () => {
		const [compact, balanced, verbose] = TOOL_OUTPUT_DETAIL_ORDER.map(
			resolveToolOutputBudgets,
		);
		expect(compact.previewMaxLines).toBeLessThan(balanced.previewMaxLines);
		expect(balanced.previewMaxLines).toBeLessThan(verbose.previewMaxLines);
		expect(compact.bodyMaxLines).toBeLessThan(verbose.bodyMaxLines);
	});
});

describe('parseToolOutputDetail', () => {
	it('accepts the three presets and falls back to balanced', () => {
		expect(parseToolOutputDetail('compact')).toBe('compact');
		expect(parseToolOutputDetail('verbose')).toBe('verbose');
		expect(parseToolOutputDetail('mystery')).toBe('balanced');
		expect(parseToolOutputDetail(null)).toBe('balanced');
		expect(parseToolOutputDetail(undefined)).toBe('balanced');
	});
});

describe('describeToolOutputBudgets', () => {
	it('summarizes the active preset', () => {
		expect(
			describeToolOutputBudgets(resolveToolOutputBudgets('balanced')),
		).toContain('10 lines');
	});
});

describe('previewFor with budgets', () => {
	it('honours compact shell budgets', () => {
		const text = Array.from({ length: 30 }, (_, i) => `out ${i}`).join('\n');
		const compact = previewFor(
			'execute_command',
			{ output: text, success: true },
			resolveToolOutputBudgets('compact'),
		);
		expect(compact.hiddenLines).toBe(26);
		const verbose = previewFor(
			'execute_command',
			{ output: text, success: true },
			resolveToolOutputBudgets('verbose'),
		);
		expect(verbose.hiddenLines).toBe(5);
	});
});
