import { client } from '#lib/api/client';
import { call } from '#lib/api/envelope';

export const ATTACHMENT_LIMIT = 64_000;
export const ATTACHMENT_MAX_COUNT = 8;
export const ATTACHMENT_TOTAL_LIMIT = 256_000;
export const IMAGE_LIMIT = 10 * 1024 * 1024;
export const IMAGE_TOTAL_LIMIT = 20 * 1024 * 1024;
export const MAX_IMAGES_PER_TURN = 8;
export const ALLOWED_IMAGE_MIMES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
export const ALLOWED_IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp'];

export interface MediaLimits {
	attachmentLimit: number;
	attachmentMaxCount: number;
	attachmentTotalLimit: number;
	imageLimit: number;
	imageTotalLimit: number;
	maxImagesPerTurn: number;
	allowedImageMimes: string[];
	allowedImageExts: string[];
}

export function builtinMediaLimits(): MediaLimits {
	return {
		attachmentLimit: ATTACHMENT_LIMIT,
		attachmentMaxCount: ATTACHMENT_MAX_COUNT,
		attachmentTotalLimit: ATTACHMENT_TOTAL_LIMIT,
		imageLimit: IMAGE_LIMIT,
		imageTotalLimit: IMAGE_TOTAL_LIMIT,
		maxImagesPerTurn: MAX_IMAGES_PER_TURN,
		allowedImageMimes: [...ALLOWED_IMAGE_MIMES],
		allowedImageExts: [...ALLOWED_IMAGE_EXTS],
	};
}

interface InfoLimitsDto {
	max_images_per_turn?: number;
	maxImagesPerTurn?: number;
	max_image_bytes?: number;
	maxImageBytes?: number;
	max_images_total_bytes?: number;
	maxImagesTotalBytes?: number;
	allowed_image_media_types?: string[];
	allowedImageMediaTypes?: string[];
	allowed_image_exts?: string[];
	allowedImageExts?: string[];
	max_attachments_per_turn?: number;
	maxAttachmentsPerTurn?: number;
	max_attachment_chars?: number;
	maxAttachmentChars?: number;
	max_attachments_total_chars?: number;
	maxAttachmentsTotalChars?: number;
}

function asPositiveInt(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) && value > 0
		? Math.floor(value)
		: null;
}

let cached: MediaLimits | null = null;

export function currentMediaLimits(): MediaLimits {
	return cached ?? builtinMediaLimits();
}

export async function loadMediaLimits(): Promise<MediaLimits> {
	if (cached) return cached;
	const fallback = builtinMediaLimits();
	try {
		const data = await call<InfoLimitsDto | null>(client.GET('/api/v1/info'));
		const envelope = (data ?? {}) as Record<string, unknown>;
		const payload = (
			typeof envelope['data'] === 'object' && envelope['data'] !== null
				? (envelope['data'] as InfoLimitsDto)
				: (envelope as InfoLimitsDto)
		) as InfoLimitsDto;
		const maxImages = asPositiveInt(payload.max_images_per_turn ?? payload.maxImagesPerTurn);
		const maxBytes = asPositiveInt(payload.max_image_bytes ?? payload.maxImageBytes);
		const maxTotal = asPositiveInt(payload.max_images_total_bytes ?? payload.maxImagesTotalBytes);
			const allowed = payload.allowed_image_media_types ?? payload.allowedImageMediaTypes;
		const allowedExts = payload.allowed_image_exts ?? payload.allowedImageExts;
		const maxFiles = asPositiveInt(
			payload.max_attachments_per_turn ?? payload.maxAttachmentsPerTurn,
		);
		const maxChars = asPositiveInt(payload.max_attachment_chars ?? payload.maxAttachmentChars);
		const maxTotalChars = asPositiveInt(
			payload.max_attachments_total_chars ?? payload.maxAttachmentsTotalChars,
		);
		const next: MediaLimits = {
			...fallback,
			...(maxImages !== null ? { maxImagesPerTurn: maxImages } : {}),
			...(maxBytes !== null ? { imageLimit: maxBytes } : {}),
			...(maxTotal !== null ? { imageTotalLimit: maxTotal } : {}),
			...(Array.isArray(allowed) && allowed.length > 0
				? {
						allowedImageMimes: allowed.filter((item) => typeof item === 'string'),
					}
				: {}),
			...(Array.isArray(allowedExts) && allowedExts.length > 0
				? {
						allowedImageExts: allowedExts
							.filter((item) => typeof item === 'string')
							.map((item) => item.toLowerCase()),
					}
				: {}),
			...(maxFiles !== null ? { attachmentMaxCount: maxFiles } : {}),
			...(maxChars !== null ? { attachmentLimit: maxChars } : {}),
			...(maxTotalChars !== null ? { attachmentTotalLimit: maxTotalChars } : {}),
		};
		cached = next;
		return next;
	} catch {
		cached = fallback;
		return fallback;
	}
}

export function resetMediaLimitsForTest(): void {
	cached = null;
}
