import { redirect } from '@sveltejs/kit';

// The execution workbench is the landing page, so the root redirects to it.
export const load = (): never => {
	redirect(307, '/executions');
};
