const PREVIEWABLE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg']);
const BITMAP_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp']);

function extensionOf(path: string): string {
	const base = path.split('/').pop() ?? path;
	const dot = base.lastIndexOf('.');
	if (dot < 0) return '';
	return base.slice(dot + 1).toLowerCase();
}

/** Workspace files the IDE can render as an image. Svg stays out of the
 * chat upload whitelist but renders here through an <img> blob URL. */
export function isPreviewableImagePath(path: string): boolean {
	return PREVIEWABLE_EXTS.has(extensionOf(path));
}

/** Svg travels as text through readFsFile, every other previewable image
 * travels as binary through readFsRaw. */
export function isSvgPath(path: string): boolean {
	return extensionOf(path) === 'svg';
}

/** Bitmap images need the binary raw endpoint. */
export function isBitmapImagePath(path: string): boolean {
	return BITMAP_EXTS.has(extensionOf(path));
}

/** Mime matching the backend mime_for_path table for previewable images. */
export function mimeForPreviewableImage(path: string): string {
	switch (extensionOf(path)) {
		case 'png':
			return 'image/png';
		case 'jpg':
		case 'jpeg':
			return 'image/jpeg';
		case 'gif':
			return 'image/gif';
		case 'webp':
			return 'image/webp';
		case 'svg':
			return 'image/svg+xml';
		default:
			return 'application/octet-stream';
	}
}

/** File name for the download attribute. */
export function fileNameOf(path: string): string {
	return path.split('/').pop() ?? path;
}

/** Directory of a workspace path; root files resolve to empty. */
export function dirnameOfWorkspacePath(path: string): string {
	const slash = path.lastIndexOf('/');
	if (slash < 0) return '';
	return path.slice(0, slash);
}

/** True when the source could name a workspace file needing blob resolution.
 * Absolute, data and pure fragment or query sources stay on the raw path. */
export function isWorkspaceRelativeImageSrc(value: unknown): boolean {
	if (!isSafeMarkdownImageSrc(value)) return false;
	const trimmed = String(value).trim();
	const lower = trimmed.toLowerCase();
	if (lower.startsWith('data:')) return false;
	if (lower.startsWith('#') || lower.startsWith('?')) return false;
	return /^[a-z][a-z0-9+.-]*:/i.test(trimmed) === false;
}

function stripImageSuffix(src: string): string {
	const hash = src.indexOf('#');
	const query = src.indexOf('?');
	let end = src.length;
	if (hash >= 0) end = Math.min(end, hash);
	if (query >= 0) end = Math.min(end, query);
	return src.slice(0, end);
}

/** Resolve a markdown image source against its document path.
 * Leading slashes anchor at the workspace root, anything else resolves
 * against the document directory. Returns null for escaping or external
 * sources so the caller falls back to the raw source. */
export function resolveWorkspaceImagePath(src: string, docPath: string): string | null {
	const clean = stripImageSuffix(src.trim());
	if (!clean) return null;
	if (/^[a-z][a-z0-9+.-]*:/i.test(clean)) return null;
	const parts: string[] = [];
	if (clean.startsWith('/')) {
		for (const part of clean.split('/')) {
			if (!part || part === '.') continue;
			if (part === '..') {
				if (parts.length === 0) return null;
				parts.pop();
				continue;
			}
			parts.push(part);
		}
	} else {
		const base = dirnameOfWorkspacePath(docPath);
		if (base) parts.push(...base.split('/').filter(Boolean));
		for (const part of clean.split('/')) {
			if (!part || part === '.') continue;
			if (part === '..') {
				if (parts.length === 0) return null;
				parts.pop();
				continue;
			}
			parts.push(part);
		}
	}
	if (parts.length === 0) return null;
	return parts.join('/');
}

function base64ToBytes(contentBase64: string): Uint8Array {
	const binary = atob(contentBase64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i += 1) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes;
}

/** Blob URL for binary payloads from readFsRaw. Caller revokes the URL. */
export function objectUrlForBase64(contentBase64: string, mime: string): string {
	const blob = new Blob([base64ToBytes(contentBase64).buffer as ArrayBuffer], {
		type: mime || 'application/octet-stream',
	});
	return URL.createObjectURL(blob);
}

/** Blob URL for svg source already loaded as text. Rendering through <img>
 * keeps embedded scripts inert, unlike inline innerHTML. Caller revokes. */
export function objectUrlForSvgText(svgText: string): string {
	const blob = new Blob([svgText], { type: 'image/svg+xml' });
	return URL.createObjectURL(blob);
}

export function revokeObjectUrl(url: string | null): void {
	if (!url) return;
	try {
		URL.revokeObjectURL(url);
	} catch {
		// Revocation is best-effort.
	}
}

/** Markdown image sources the chat may render. Mirrors the parser policy:
 * http/https, relative paths, and bitmap data URLs. Svg data URLs stay
 * blocked because svg can carry scripts. */
export function isSafeMarkdownImageSrc(value: unknown): boolean {
	if (typeof value !== 'string') return false;
	const trimmed = value.trim();
	if (!trimmed) return false;
	const lower = trimmed.toLowerCase();
	if (lower.startsWith('data:')) {
		return /^data:image\/(png|gif|jpe?g|webp|avif|bmp);/i.test(trimmed);
	}
	if (
		lower.startsWith('#') ||
		lower.startsWith('/') ||
		lower.startsWith('./') ||
		lower.startsWith('../') ||
		lower.startsWith('?')
	) {
		return true;
	}
	const scheme = lower.match(/^([a-z][a-z0-9+.-]*):/)?.[1] ?? '';
	if (!scheme) return true;
	return scheme === 'http' || scheme === 'https';
}
