import { describe, expect, it } from 'vitest';
import {
	fileNameOf,
	isBitmapImagePath,
	isPreviewableImagePath,
	isSafeMarkdownImageSrc,
	isSvgPath,
	mimeForPreviewableImage,
} from './image-preview';

describe('image preview paths', () => {
	it('accepts bitmap and svg files case-insensitively', () => {
		expect(isPreviewableImagePath('a.PNG')).toBe(true);
		expect(isPreviewableImagePath('dir/b.svg')).toBe(true);
		expect(isPreviewableImagePath('c.webp')).toBe(true);
		expect(isPreviewableImagePath('d.pdf')).toBe(false);
		expect(isPreviewableImagePath('noext')).toBe(false);
	});

	it('separates svg text from bitmap binaries', () => {
		expect(isSvgPath('a.svg')).toBe(true);
		expect(isSvgPath('A.SVG')).toBe(true);
		expect(isSvgPath('a.png')).toBe(false);
		expect(isBitmapImagePath('a.jpg')).toBe(true);
		expect(isBitmapImagePath('a.svg')).toBe(false);
	});

	it('maps mime types like the backend', () => {
		expect(mimeForPreviewableImage('a.svg')).toBe('image/svg+xml');
		expect(mimeForPreviewableImage('a.jpg')).toBe('image/jpeg');
		expect(mimeForPreviewableImage('a.png')).toBe('image/png');
	});

	it('derives file names', () => {
		expect(fileNameOf('a/b/c.png')).toBe('c.png');
	});
});

describe('markdown image sources', () => {
	it('allows http, relative and bitmap data urls', () => {
		expect(isSafeMarkdownImageSrc('https://example.com/a.png')).toBe(true);
		expect(isSafeMarkdownImageSrc('./a.png')).toBe(true);
		expect(isSafeMarkdownImageSrc('a/b.png')).toBe(true);
		expect(isSafeMarkdownImageSrc('data:image/png;base64,AAA')).toBe(true);
	});

	it('blocks scripts and svg data urls', () => {
		expect(isSafeMarkdownImageSrc('javascript:alert(1)')).toBe(false);
		expect(isSafeMarkdownImageSrc('data:image/svg+xml;base64,AAA')).toBe(false);
		expect(isSafeMarkdownImageSrc('data:text/html,hi')).toBe(false);
		expect(isSafeMarkdownImageSrc('')).toBe(false);
	});
});
