export interface ConflictBlock {
	index: number;
	startLine: number;
	endLine: number;
	oursStart: number;
	oursEnd: number;
	theirsStart: number;
	theirsEnd: number;
	oursLines: string[];
	theirsLines: string[];
}

type SourceLine = string | { text: string };

function lineText(line: SourceLine): string {
	return typeof line === 'string' ? line : (line.text ?? '');
}

function isStart(text: string): boolean {
	return text.startsWith('<<<<<<<');
}

function isBase(text: string): boolean {
	return text.startsWith('|||||||');
}

function isMid(text: string): boolean {
	return text === '=======' || text.startsWith('======= ') || text.startsWith('=======\t');
}

function isEnd(text: string): boolean {
	return text.startsWith('>>>>>>>');
}

/**
 * Parse conflict marker blocks from already loaded diff rows.
 *
 * Each block reports its own line interval plus the ours/theirs intervals
 * and texts. Incomplete marker sets are ignored so a partially written
 * file never yields a phantom block.
 */
export function parseConflictBlocks(lines: SourceLine[]): ConflictBlock[] {
	const texts = lines.map(lineText);
	const blocks: ConflictBlock[] = [];
	let index = 0;
	while (index < texts.length) {
		if (!isStart(texts[index])) {
			index += 1;
			continue;
		}
		const start = index;
		let base: number | null = null;
		let mid: number | null = null;
		let end: number | null = null;
		let cursor = index + 1;
		while (cursor < texts.length) {
			const text = texts[cursor];
			if (isBase(text) && base === null && mid === null) base = cursor;
			else if (isMid(text) && mid === null) mid = cursor;
			else if (isEnd(text) && mid !== null) {
				end = cursor;
				break;
			} else if (isStart(text)) break;
			cursor += 1;
		}
		if (mid === null || end === null) {
			index = start + 1;
			continue;
		}
		const oursEnd = base ?? mid;
		const oursLines = texts.slice(start + 1, oursEnd);
		const theirsLines = texts.slice(mid + 1, end);
		blocks.push({
			index: blocks.length,
			startLine: start + 1,
			endLine: end + 1,
			oursStart: oursLines.length > 0 ? start + 2 : start + 1,
			oursEnd: Math.max(start + 1, oursEnd),
			theirsStart: theirsLines.length > 0 ? mid + 2 : mid + 1,
			theirsEnd: Math.max(mid + 1, end),
			oursLines,
			theirsLines,
		});
		index = end + 1;
	}
	return blocks;
}
