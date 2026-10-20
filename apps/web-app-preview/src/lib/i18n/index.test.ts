import { describe, expect, it } from 'vitest';
import { getLocale, t } from './index.js';

describe('i18n entry', () => {
	it('renders a message in the active locale', () => {
		expect(t('common.cancel')).toBe('Cancel');
	});

	it('interpolates values into messages', () => {
		expect(t('common.items', { count: 3 })).toBe('3 items');
	});

	it('exposes a locale from the supported set', () => {
		expect(['en', 'zh']).toContain(getLocale());
	});
});
