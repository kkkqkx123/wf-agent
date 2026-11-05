/** Mock routes: the server-side preference document.
 *
 * The document is a flat `{ option-key: value }` map; GET returns it whole
 * and PUT replaces it verbatim, mirroring the backend contract. The mock
 * keeps values in module memory for the preview session so saves survive
 * reloads within the tab but never touch a real store.
 */

import type { MockRouter } from '../router';

const preferences: Record<string, unknown> = {
	'execution.page-size': 50,
	'execution.stream-follow': true,
	'appearance.reduce-motion': false,
};

export function registerPreferences(router: MockRouter): void {
	router.on('GET', '/api/v1/preferences', () => preferences);

	router.on('PUT', '/api/v1/preferences', (_params, _query, body) => {
		const values = (body ?? {}) as { values?: Record<string, unknown> };
		if (values.values && typeof values.values === 'object') {
			// The backend replaces the document verbatim.
			for (const key of Object.keys(preferences)) delete preferences[key];
			Object.assign(preferences, values.values);
		}
		return preferences;
	});
}
