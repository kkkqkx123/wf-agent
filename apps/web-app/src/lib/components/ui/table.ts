import type { Snippet } from 'svelte';

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
	/** Extra classes for every cell in this column, e.g. monospace or numeric. */
	cellClass?: string;
	/**
	 * Narrow-screen card placement. The first column becomes the card title
	 * and the rest become detail rows unless marked otherwise; columns
	 * marked as actions render in the card footer instead.
	 */
	card?: 'title' | 'detail' | 'actions';
}
