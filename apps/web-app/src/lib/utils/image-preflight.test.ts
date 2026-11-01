import { describe, expect, it } from 'vitest';
import { builtinMediaLimits, type MediaLimits } from '#lib/config/media';
import {
	imageExtMime,
	imageMimeAllowed,
	preflightImage,
} from '#lib/utils/image-preflight';

const limits: MediaLimits = builtinMediaLimits();

function input(overrides: Partial<Parameters<typeof preflightImage>[0]> = {}) {
	return {
		mime: 'image/png',
		name: 'shot.png',
		size: 100,
		stagedCount: 0,
		stagedBytes: 0,
		...overrides,
	};
}

describe('imageMimeAllowed', () => {
	it('accepts whitelisted mime types case-insensitively', () => {
		expect(imageMimeAllowed('image/PNG', 'a.png', limits)).toBe(true);
		expect(imageMimeAllowed('image/webp', 'a.webp', limits)).toBe(true);
	});

	it('falls back to the file extension when the mime is unknown', () => {
		expect(imageMimeAllowed('', 'photo.JPG', limits)).toBe(true);
		expect(imageMimeAllowed('', 'photo.jpeg', limits)).toBe(true);
	});

	it('rejects types outside the whitelist even when the browser says image/', () => {
		expect(imageMimeAllowed('image/svg+xml', 'logo.svg', limits)).toBe(false);
		expect(imageMimeAllowed('image/bmp', 'a.bmp', limits)).toBe(false);
		expect(imageMimeAllowed('', 'a.bmp', limits)).toBe(false);
	});
});

describe('imageExtMime', () => {
	it('keeps the browser-reported type when present', () => {
		expect(imageExtMime('image/png', 'a.png')).toBe('image/png');
	});

	it('maps known extensions and leaves unknown ones undefined', () => {
		expect(imageExtMime('', 'a.jpg')).toBe('image/jpeg');
		expect(imageExtMime('', 'a.webp')).toBe('image/webp');
		expect(imageExtMime('', 'a.bin')).toBeUndefined();
	});
});

describe('preflightImage', () => {
	it('accepts an in-budget image', () => {
		expect(preflightImage(input(), limits)).toBeNull();
	});

	it('rejects unsupported types before anything else', () => {
		expect(preflightImage(input({ mime: 'image/svg+xml', name: 'logo.svg' }), limits)).toBe(
			'unsupported-type',
		);
		expect(preflightImage(input({ mime: '', name: 'a.bmp' }), limits)).toBe('unsupported-type');
	});

	it('rejects staging beyond the per-turn count', () => {
		expect(
			preflightImage(input({ stagedCount: limits.maxImagesPerTurn }), limits),
		).toBe('too-many');
	});

	it('rejects a single file over the per-image limit', () => {
		expect(preflightImage(input({ size: limits.imageLimit + 1 }), limits)).toBe('too-large');
	});

	it('rejects when the per-turn budget would be exceeded', () => {
		expect(
			preflightImage(
				input({ stagedBytes: limits.imageTotalLimit - 50, size: 100 }),
				limits,
			),
		).toBe('total-too-large');
	});
});
