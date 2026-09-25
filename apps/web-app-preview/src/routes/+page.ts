import { redirect } from '@sveltejs/kit';

// The dialogue-first IDE is the landing page, so the root redirects to it.
export const load = (): never => {
	redirect(307, '/chat');
};
