/** Rendering contract for the IDE file tree windowing step. */

export const IDE_TREE_ROW_HEIGHT_PX = 28;

export interface FileTreeWindowProps<T> {
	dirPath: string;
	level: number;
	rowHeightPx: typeof IDE_TREE_ROW_HEIGHT_PX;
	items: T[];
}

/** Whether a directory should swap its render container for a windowed list. */
export function shouldVirtualizeRow(count: number, threshold = 500): boolean {
	return count >= threshold;
}
