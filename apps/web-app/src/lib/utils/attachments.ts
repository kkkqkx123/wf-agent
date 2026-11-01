import type { MessageAttachment } from '#lib/types/models';

const BLOCK_PATTERN = /<attachment name="([^"]*)">\n([\s\S]*?)\n<\/attachment>/g;

function escapeAttachmentName(name: string): string {
	return name.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

function unescapeAttachmentName(name: string): string {
	return name.replace(/&quot;/g, '"').replace(/&amp;/g, '&');
}

function escapeAttachmentContent(content: string): string {
	return content
		.replace(/\\/g, '\\\\')
		.replace(/<attachment /g, '<\\attachment ')
		.replace(/<\/attachment>/g, '<\\/attachment>');
}

function unescapeAttachmentContent(content: string): string {
	return content
		.replace(/<\\\/attachment>/g, '</attachment>')
		.replace(/<\\attachment /g, '<attachment ')
		.replace(/\\\\/g, '\\');
}

/**
 * Attachments travel inside the message body because the run endpoint takes a
 * single text field; the view layer reads them back out with `splitAttachments`.
 *
 * Flattened format (stable contract): `[text, ...blocks].join('\n\n')` where each
 * block is `<attachment name="<escaped>">\n<escaped>\n</attachment>`. Attachment
 * payloads escape backslashes and block markers so nested markers never split;
 * the leading text stays verbatim. Parsers keep unparsed text visible and never
 * drop it.
 */
export function withAttachments(text: string, attachments: MessageAttachment[]): string {
	if (attachments.length === 0) return text;
	const blocks = attachments.map(
		(file) =>
			`<attachment name="${escapeAttachmentName(file.name)}">\n${escapeAttachmentContent(file.content)}\n</attachment>`,
	);
	if (!text) return blocks.join('\n\n');
	return [text, ...blocks].join('\n\n');
}

/** Anything the blocks do not parse as stays visible text, never dropped. Only
 * join newlines are stripped; user spaces are preserved. */
export function splitAttachments(text: string): {
	text: string;
	attachments: MessageAttachment[];
} {
	const attachments: MessageAttachment[] = [];
	const body = text.replace(BLOCK_PATTERN, (_match, name: string, content: string) => {
		attachments.push({
			name: unescapeAttachmentName(name),
			content: unescapeAttachmentContent(content),
		});
		return '';
	});
	return { text: body.replace(/^\n+/, '').replace(/\n+$/, ''), attachments };
}
