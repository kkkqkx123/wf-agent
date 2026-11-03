import type { MessageAttachment } from '#lib/types/models';

const BLOCK_PATTERN =
	/<attachment name="([^"]*)"(?:\s+truncated="(true|false)")?(?:\s+original="(\d+)")?>\n([\s\S]*?)\n<\/attachment>/g;

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
 * Attachments travel inside the message body for history compatibility;
 * new turns prefer the structured `attachments` channel and the server
 * re-inlines them with truncation attributes for storage. The view layer
 * reads both old and new blocks back out with `splitAttachments`.
 *
 * Flattened format (stable contract): `[text, ...blocks].join('\n\n')` where
 * each block is `<attachment name="<escaped>"[ truncated="true" original="N"]>\n<escaped>\n</attachment>`.
 * Payloads escape backslashes and block markers so nested markers never
 * split; the leading text stays verbatim. Parsers keep unparsed text
 * visible and never drop it. Old blocks without attributes still parse as
 * untruncated for history compatibility.
 */
export function withAttachments(text: string, attachments: MessageAttachment[]): string {
	if (attachments.length === 0) return text;
	const blocks = attachments.map((file) => {
		const attrs =
			(file.truncated ? ' truncated="true"' : '') +
			(typeof file.originalChars === 'number' ? ` original="${file.originalChars}"` : '');
		return `<attachment name="${escapeAttachmentName(file.name)}"${attrs}>\n${escapeAttachmentContent(file.content)}\n</attachment>`;
	});
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
	const body = text.replace(
		BLOCK_PATTERN,
		(
			_match: string,
			name: string,
			truncated: string | undefined,
			original: string | undefined,
			content: string,
		) => {
			const entry: MessageAttachment = {
				name: unescapeAttachmentName(name),
				content: unescapeAttachmentContent(content),
			};
			if (truncated === 'true') entry.truncated = true;
			if (original !== undefined) {
				const parsed = Number.parseInt(original, 10);
				if (Number.isFinite(parsed) && parsed >= 0) entry.originalChars = parsed;
			}
			attachments.push(entry);
			return '';
		},
	);
	return { text: body.replace(/^\n+/, '').replace(/\n+$/, ''), attachments };
}
