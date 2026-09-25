import type { MessageAttachment } from '$lib/types/models';

const BLOCK_PATTERN =
	/<attachment name="([^"]*)">\n([\s\S]*?)\n<\/attachment>/g;

/**
 * Attachments travel inside the message body because the run endpoint takes a
 * single text field; the view layer reads them back out with `splitAttachments`.
 */
export function withAttachments(
	text: string,
	attachments: MessageAttachment[],
): string {
	if (attachments.length === 0) return text;
	const blocks = attachments.map(
		(file) =>
			`<attachment name="${file.name}">\n${file.content}\n</attachment>`,
	);
	return [text, ...blocks].join('\n\n');
}

/** Anything the blocks do not parse as stays visible text, never dropped. */
export function splitAttachments(text: string): {
	text: string;
	attachments: MessageAttachment[];
} {
	const attachments: MessageAttachment[] = [];
	const body = text.replace(
		BLOCK_PATTERN,
		(_match, name: string, content: string) => {
			attachments.push({ name, content });
			return '';
		},
	);
	return { text: body.trim(), attachments };
}
