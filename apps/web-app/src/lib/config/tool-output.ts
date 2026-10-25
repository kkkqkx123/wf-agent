/**
 * Windowed tool-output budgets. Every tool-output surface reads its preview
 * limits here so live, history and audit views collapse alike. The balanced
 * magnitudes track the TUI defaults; only the expanded body cap is
 * web-specific and bounds the inline DOM size of one tool block.
 *
 * Budgets are exposed as named presets rather than free numbers: line and
 * character counts are unpredictable in a settings UI, and unconstrained
 * values could hide failures. The detail preference selects one preset; the
 * retrieval-hidden rule and the error floor hold across all presets.
 *
 * The server display payload carries up to 25 lines, the verbose ceiling:
 * presets only cut below it, never above.
 */

/** Detail preset selected in Settings. Balanced matches the TUI defaults. */
export type ToolOutputDetail = 'compact' | 'balanced' | 'verbose';

/** One preset bundle: all row and character budgets for tool previews. */
export interface ToolOutputBudgets {
	/** Generic preview: head lines kept. */
	previewMaxLines: number;
	/** Generic preview: grapheme budget applied after the line cut. */
	previewMaxChars: number;
	/** Shell preview: tail lines kept. */
	shellPreviewLines: number;
	/** Failure preview: head lines of the error text kept. Never below 2:
	 * failures stay diagnosable even on the most compact preset. */
	errorPreviewLines: number;
	/** Expanded inline body: head lines kept before the omission marker. */
	bodyMaxLines: number;
}

/** Generic preview: head lines kept. */
export const TOOL_PREVIEW_MAX_LINES = 10;
/** Generic preview: grapheme budget applied after the line cut. */
export const TOOL_PREVIEW_MAX_CHARS = 320;
/** Shell preview: tail lines kept. */
export const TOOL_SHELL_PREVIEW_LINES = 10;
/** Failure preview: head lines of the error text kept. */
export const TOOL_ERROR_PREVIEW_LINES = 4;
/** Expanded inline body: head lines kept before the omission marker. */
export const TOOL_BODY_MAX_LINES = 300;

export const TOOL_OUTPUT_PRESETS: Record<ToolOutputDetail, ToolOutputBudgets> = {
	compact: {
		previewMaxLines: 4,
		previewMaxChars: 160,
		shellPreviewLines: 4,
		errorPreviewLines: 2,
		bodyMaxLines: 100,
	},
	balanced: {
		previewMaxLines: TOOL_PREVIEW_MAX_LINES,
		previewMaxChars: TOOL_PREVIEW_MAX_CHARS,
		shellPreviewLines: TOOL_SHELL_PREVIEW_LINES,
		errorPreviewLines: TOOL_ERROR_PREVIEW_LINES,
		bodyMaxLines: TOOL_BODY_MAX_LINES,
	},
	verbose: {
		previewMaxLines: 25,
		previewMaxChars: 800,
		shellPreviewLines: 25,
		errorPreviewLines: 8,
		bodyMaxLines: 600,
	},
};

export const TOOL_OUTPUT_DETAIL_ORDER: ToolOutputDetail[] = ['compact', 'balanced', 'verbose'];

/** Parse a persisted detail value; unknown values fall back to balanced so
 * a corrupt preference never breaks rendering. */
export function parseToolOutputDetail(value: unknown): ToolOutputDetail {
	if (value === 'compact' || value === 'balanced' || value === 'verbose') {
		return value;
	}
	return 'balanced';
}

/** Budgets for one detail preset. */
export function resolveToolOutputBudgets(detail: ToolOutputDetail): ToolOutputBudgets {
	return TOOL_OUTPUT_PRESETS[detail];
}

/** One-line summary of a preset for settings hints, e.g. `10 lines · 320 chars`. */
export function describeToolOutputBudgets(budgets: ToolOutputBudgets): string {
	return `${budgets.previewMaxLines} lines · ${budgets.previewMaxChars} chars · ${budgets.bodyMaxLines} expanded`;
}
