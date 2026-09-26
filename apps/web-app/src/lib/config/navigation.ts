import type { IconName } from '$lib/components/icons/paths';
import type { AppPath } from '$lib/utils/route';

export interface NavItem {
	href: AppPath;
	label: string;
	icon: IconName;
	description: string;
	/** Segments rendered as sub-tabs instead of separate top level routes. */
	segments?: Array<{ id: string; label: string; icon: IconName }>;
}

export interface NavGroup {
	id: string;
	label: string;
	items: NavItem[];
}

export const NAV_GROUPS: NavGroup[] = [
	{
		id: 'execution',
		label: 'Execution',
		items: [
			{
				href: '/executions',
				label: 'Workbench',
				icon: 'activity',
				description: 'Execution list, status and live detail',
			},
			{
				href: '/agent-loops',
				label: 'Agent Loops',
				icon: 'loop',
				description: 'Loop runs, messages, variables and checkpoints',
			},
		],
	},
	{
		id: 'orchestration',
		label: 'Orchestration',
		items: [
			{
				href: '/workflows',
				label: 'Workflows',
				icon: 'workflow',
				description: 'Definitions, versions, drafts and graph',
			},
			{
				href: '/templates',
				label: 'Templates',
				icon: 'template',
				description: 'Node, trigger and agent template registry',
			},
		],
	},
	{
		id: 'state',
		label: 'State',
		items: [
			{
				href: '/checkpoints',
				label: 'Checkpoints',
				icon: 'archive',
				description: 'Execution chains, file workspace and approvals',
			},
		],
	},
	{
		id: 'integration',
		label: 'Integration',
		items: [
			{
				href: '/triggers',
				label: 'Triggers',
				icon: 'zap',
				description: 'Trigger records and hook test dispatch',
			},
			{
				href: '/resources',
				label: 'Models & Tools',
				icon: 'cpu',
				description: 'Profiles, providers, tools, scripts and skills',
				segments: [
					{ id: 'models', label: 'Models', icon: 'sparkles' },
					{ id: 'tools', label: 'Tools', icon: 'blocks' },
					{ id: 'scripts', label: 'Scripts', icon: 'code' },
					{ id: 'skills', label: 'Skills', icon: 'star' },
				],
			},
		],
	},
	{
		id: 'insight',
		label: 'Insight',
		items: [
			{
				href: '/insights',
				label: 'Query & Audit',
				icon: 'chart',
				description: 'Ad-hoc query, audit reports, errors and performance',
				segments: [
					{ id: 'query', label: 'Query', icon: 'search' },
					{ id: 'audit', label: 'Audit', icon: 'file' },
					{ id: 'errors', label: 'Errors', icon: 'alert-triangle' },
					{ id: 'performance', label: 'Performance', icon: 'gauge' },
				],
			},
			{
				href: '/events',
				label: 'Events & System',
				icon: 'radio',
				description: 'Event stream, dependencies and runtime diagnostics',
				segments: [
					{ id: 'stream', label: 'Stream', icon: 'radio' },
					{ id: 'dependencies', label: 'Dependencies', icon: 'layers' },
					{ id: 'operations', label: 'Operations', icon: 'shield' },
				],
			},
		],
	},
	{
		id: 'workspace',
		label: 'Workspace',
		items: [
			{
				href: '/settings',
				label: 'Settings',
				icon: 'sliders',
				description: 'Appearance, execution defaults and notifications',
			},
		],
	},
];

export const NAV_ITEMS: NavItem[] = NAV_GROUPS.flatMap((group) => group.items);

export function findNavItem(href: string): NavItem | undefined {
	return NAV_ITEMS.find(
		(item) => href === item.href || href.startsWith(`${item.href}/`),
	);
}

export function navItemFor(pathname: string): NavItem | undefined {
	const match = NAV_ITEMS.filter(
		(item) => pathname === item.href || pathname.startsWith(`${item.href}/`),
	);
	return match.sort((a, b) => b.href.length - a.href.length)[0];
}
