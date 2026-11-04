import { describe, expect, it } from 'vitest';
import { builtinMediaLimits, currentMediaLimits, resetMediaLimitsForTest } from './media';

describe('media limits', () => {
	it('exposes attachment budgets matching the server validation', () => {
		resetMediaLimitsForTest();
		const limits = builtinMediaLimits();
		expect(limits.attachmentMaxCount).toBe(8);
		expect(limits.attachmentLimit).toBe(64000);
		expect(limits.attachmentTotalLimit).toBe(256000);
	});

	it('exposes builtin limits matching the server validation', () => {
		resetMediaLimitsForTest();
		const limits = builtinMediaLimits();
		expect(limits.maxImagesPerTurn).toBe(8);
		expect(limits.imageLimit).toBe(10 * 1024 * 1024);
		expect(limits.imageTotalLimit).toBe(20 * 1024 * 1024);
		expect(limits.allowedImageMimes).toEqual([
			'image/png',
			'image/jpeg',
			'image/gif',
			'image/webp',
		]);
		expect(limits.allowedImageExts).toEqual(['png', 'jpg', 'jpeg', 'gif', 'webp']);
		expect(currentMediaLimits()).toEqual(limits);
	});
});
