import { describe, expect, it } from 'vitest';
import {
	dirnameOfWorkspacePath,
	fileNameOf,
	isBitmapImagePath,
	isPreviewableImagePath,
	isSafeMarkdownImageSrc,
	isSvgPath,
	isWorkspaceRelativeImageSrc,
	mimeForPreviewableImage,
	resolveWorkspaceImagePath,
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

describe('workspace-relative image resolution', () => {
	it('resolves relative sources against the document directory', () => {
		expect(resolveWorkspaceImagePath('./a.png', 'docs/notes.md')).toBe('docs/a.png');
		expect(resolveWorkspaceImagePath('../assets/i.svg', 'docs/notes.md')).toBe('assets/i.svg');
		expect(resolveWorkspaceImagePath('a/b.png', 'notes.md')).toBe('a/b.png');
	});

	it('anchors leading slashes at the workspace root', () => {
		expect(resolveWorkspaceImagePath('/assets/i.png', 'docs/deep/notes.md')).toBe('assets/i.png');
	});

	it('strips query and fragment suffixes before resolving', () => {
		expect(resolveWorkspaceImagePath('a.png?raw=true#frag', 'docs/notes.md')).toBe('docs/a.png');
	});

	it('rejects escaping and external sources', () => {
		expect(resolveWorkspaceImagePath('../../etc/passwd', 'docs/notes.md')).toBeNull();
		expect(resolveWorkspaceImagePath('../a.png', 'notes.md')).toBeNull();
		expect(resolveWorkspaceImagePath('https://example.com/a.png', 'docs/notes.md')).toBeNull();
		expect(resolveWorkspaceImagePath('', 'docs/notes.md')).toBeNull();
	});

	it('classifies which sources need workspace resolution', () => {
		expect(isWorkspaceRelativeImageSrc('./a.png')).toBe(true);
		expect(isWorkspaceRelativeImageSrc('/a.png')).toBe(true);
		expect(isWorkspaceRelativeImageSrc('a/b.png')).toBe(true);
		expect(isWorkspaceRelativeImageSrc('https://example.com/a.png')).toBe(false);
		expect(isWorkspaceRelativeImageSrc('data:image/png;base64,AAA')).toBe(false);
		expect(isWorkspaceRelativeImageSrc('#frag')).toBe(false);
		expect(isWorkspaceRelativeImageSrc('javascript:alert(1)')).toBe(false);
	});

	it('derives document directories', () => {
		expect(dirnameOfWorkspacePath('docs/notes.md')).toBe('docs');
		expect(dirnameOfWorkspacePath('notes.md')).toBe('');
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
