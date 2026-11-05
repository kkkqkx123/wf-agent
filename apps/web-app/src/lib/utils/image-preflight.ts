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
	/** File size in decoded bytes. */
	size: number;
	/** Images already staged this turn. */
	stagedCount: number;
	/** Sum of staged payload sizes in decoded bytes this turn. */
	stagedBytes: number;
}

/** Decoded length of a base64 payload without allocating the bytes.
 * Remote addresses carry unknown sizes and report zero. */
export function base64DecodedLength(payload: string): number {
	let sextets = 0;
	let padding = 0;
	for (const char of payload) {
		if (char === ' ' || char === '\n' || char === '\r' || char === '\t') continue;
		if (
			(char >= 'A' && char <= 'Z') ||
			(char >= 'a' && char <= 'z') ||
			(char >= '0' && char <= '9') ||
			char === '+' ||
			char === '/'
		) {
			if (padding > 0) return 0;
			sextets += 1;
		} else if (char === '=') {
			padding += 1;
		} else {
			return 0;
		}
	}
	if (padding > 2 || (sextets + padding) % 4 !== 0) return 0;
	const total = sextets + padding;
	if (total === 0) return 0;
	return (total / 4) * 3 - padding;
}

/** Decoded image bytes of one staged data URL in the same unit the server
 * validates. Remote addresses report zero because their size is unknown
 * before download. */
export function dataUrlDecodedBytes(dataUrl: string): number {
	if (!dataUrl.startsWith('data:')) return 0;
	const comma = dataUrl.indexOf(',');
	if (comma < 0) return 0;
	return base64DecodedLength(dataUrl.slice(comma + 1));
}

/** Decoded image bytes across staged images for budget display. */
export function stagedDecodedBytes(images: { dataUrl: string }[]): number {
	return images.reduce((total, image) => total + dataUrlDecodedBytes(image.dataUrl), 0);
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
