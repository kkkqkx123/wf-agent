import type { Snippet } from 'svelte';

/** Row count above which DataTable switches to windowed rendering. */
export const VIRTUALIZE_THRESHOLD = 200;

/**
 * Column descriptor for DataTable. Use `text` for plain values and `cell` when
 * a row needs richer markup; `cell` wins when both are provided.
 */
export interface Column<T> {
	key: string;
	header: string;
	align?: 'left' | 'right' | 'center';
	width?: string;
	text?: (row: T) => string;
	cell?: Snippet<[T]>;
}
