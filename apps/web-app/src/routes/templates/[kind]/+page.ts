import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import type { PageLoad } from './$types';

/**
 * `/templates/[kind]` exists only as a path segment for the detail route below
 * it. Visiting it directly used to 404, so send bare kind URLs back to the list
 * page, which keeps the selected kind in its `tab` parameter.
 */
export const load: PageLoad = ({ params }) => {
	redirect(
		307,
		`${resolve('/templates')}?tab=${encodeURIComponent(params.kind)}`,
	);
};
