import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

/** Legacy surface: every agent loop execution lives in the unified
 * workbench now, pre-filtered to the loop engine. */
export const load: PageLoad = () => {
	redirect(308, '/executions?engine=agent_loop');
};
