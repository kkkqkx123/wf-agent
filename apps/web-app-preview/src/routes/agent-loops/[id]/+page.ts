import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

/** Legacy loop detail tab → unified execution tab mapping. */
const TAB_MAP: Record<string, string> = {
	messages: 'messages',
	variables: 'variables',
	graph: 'graph',
	analysis: 'analysis',
	checkpoints: 'checkpoints',
};

export const load: PageLoad = ({ params, url }) => {
	const tab = TAB_MAP[url.searchParams.get('tab') ?? ''];
	const suffix = tab ? `?tab=${tab}` : '';
	redirect(308, `/executions/${params.id}${suffix}`);
};
