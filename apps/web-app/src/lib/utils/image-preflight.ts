import { currentMediaLimits, type MediaLimits } from '#lib/config/media';

/**
 * Pure preflight rules for staging one image file: the shared checks behind
 * the file picker, paste and drag-drop paths. Errors are symbolic so callers
 * own the wording (toast or inline); a null result means the file may be
 * staged and read.
 */
export type ImagePreflightError =
	/** Mime and extension both fall outside the server whitelist. */
	| 'unsupported-type'
	/** The per-turn image count is already exhausted. */
	| 'too-many'
	/** This one file exceeds the per-image byte limit. */
	| 'too-large'
	/** Staging this file would exceed the per-turn byte budget. */
	| 'total-too-large';

export interface ImagePreflightInput {
	/** Browser-reported MIME type; empty when unknown. */
	mime: string;
	/** File name used for the extension fallback check. */
	name: string;
	/** File size in bytes. */
	size: number;
	/** Images already staged this turn. */
	stagedCount: number;
	/** Sum of staged payload sizes (dataUrl lengths) this turn. */
	stagedBytes: number;
}

/** Whether the media type or file extension passes the whitelist. */
export function imageMimeAllowed(
	mime: string,
	name: string,
	limits: MediaLimits = currentMediaLimits(),
): boolean {
	if (mime && limits.allowedImageMimes.includes(mime.toLowerCase())) return true;
	const ext = (name.split('.').pop() ?? '').toLowerCase();
	return limits.allowedImageExts.includes(ext);
}

/** Map a file extension to the inline media type sent to the server; falls
 * back to the browser-reported type when the extension is unknown. */
export function imageExtMime(mime: string, name: string): string | undefined {
	if (mime) return mime;
	const ext = (name.split('.').pop() ?? '').toLowerCase();
	switch (ext) {
		case 'png':
			return 'image/png';
		case 'jpg':
		case 'jpeg':
			return 'image/jpeg';
		case 'gif':
			return 'image/gif';
		case 'webp':
			return 'image/webp';
		default:
			return undefined;
	}
}

/** Run every staging gate in order; the first violation names the failure. */
export function preflightImage(
	input: ImagePreflightInput,
	limits: MediaLimits = currentMediaLimits(),
): ImagePreflightError | null {
	if (!imageMimeAllowed(input.mime, input.name, limits)) return 'unsupported-type';
	if (input.stagedCount >= limits.maxImagesPerTurn) return 'too-many';
	if (input.size > limits.imageLimit) return 'too-large';
	if (input.stagedBytes + input.size > limits.imageTotalLimit) return 'total-too-large';
	return null;
}
