import { message } from '#lib/i18n';
import type { IconName } from '#lib/ui/icons/paths';
import type { AppPath } from '#lib/utils/route';

export interface NavItem {
	href: AppPath;
	label: () => string;
	icon: IconName;
	description: () => string;
	/** Segments rendered as sub-tabs instead of separate top level routes. */
	segments?: Array<{ id: string; label: () => string; icon: IconName }>;
}

export interface NavGroup {
	id: string;
	label: () => string;
	items: NavItem[];
}

export const NAV_GROUPS: NavGroup[] = [
	{
		id: 'run',
		label: message('config.navigation.run'),
		items: [
			{
				href: '/chat',
				label: message('config.navigation.chat'),
				icon: 'sparkles',
				description: message('config.navigation.chatDescription'),
			},
			{
				href: '/executions',
				label: message('config.navigation.executions'),
				icon: 'activity',
				description: message('config.navigation.executionsDescription'),
			},
			{
				href: '/triggers',
				label: message('config.navigation.triggers'),
				icon: 'zap',
				description: message('config.navigation.triggersDescription'),
			},
			{
				href: '/dashboard',
				label: message('config.navigation.dashboard'),
				icon: 'gauge',
				description: message('config.navigation.dashboardDescription'),
			},
		],
	},
	{
		id: 'build',
		label: message('config.navigation.build'),
		items: [
			{
				href: '/templates',
				label: message('config.navigation.templates'),
				icon: 'template',
				description: message('config.navigation.templatesDescription'),
			},
			{
				href: '/resources',
				label: message('config.navigation.resources'),
				icon: 'cpu',
				description: message('config.navigation.resourcesDescription'),
				segments: [
					{
						id: 'models',
						label: message('config.navigation.models'),
						icon: 'sparkles',
					},
					{
						id: 'tools',
						label: message('config.navigation.tools'),
						icon: 'blocks',
					},
					{
						id: 'scripts',
						label: message('config.navigation.scripts'),
						icon: 'code',
					},
					{
						id: 'skills',
						label: message('config.navigation.skills'),
						icon: 'star',
					},
				],
			},
		],
	},
	{
		id: 'develop',
		label: message('config.navigation.develop'),
		items: [
			{
				href: '/ide',
				label: message('config.navigation.ide'),
				icon: 'code',
				description: message('config.navigation.ideDescription'),
			},
			{
				href: '/git',
				label: message('config.navigation.git'),
				icon: 'git-commit',
				description: message('config.navigation.gitDescription'),
			},
			{
				href: '/terminal',
				label: message('config.navigation.terminal'),
				icon: 'terminal',
				description: message('config.navigation.terminalDescription'),
			},
		],
	},
	{
		id: 'observe',
		label: message('config.navigation.observe'),
		items: [
			{
				href: '/checkpoints',
				label: message('config.navigation.checkpoints'),
				icon: 'archive',
				description: message('config.navigation.checkpointsDescription'),
			},
			{
				href: '/insights',
				label: message('config.navigation.insights'),
				icon: 'chart',
				description: message('config.navigation.insightsDescription'),
				segments: [
					{
						id: 'query',
						label: message('config.navigation.query'),
						icon: 'search',
					},
					{
						id: 'audit',
						label: message('config.navigation.audit'),
						icon: 'file',
					},
					{
						id: 'errors',
						label: message('config.navigation.errors'),
						icon: 'alert-triangle',
					},
					{
						id: 'performance',
						label: message('config.navigation.performance'),
						icon: 'gauge',
					},
				],
			},
			{
				href: '/events',
				label: message('config.navigation.events'),
				icon: 'radio',
				description: message('config.navigation.eventsDescription'),
				segments: [
					{
						id: 'stream',
						label: message('config.navigation.stream'),
						icon: 'radio',
					},
					{
						id: 'dependencies',
						label: message('config.navigation.dependencies'),
						icon: 'layers',
					},
					{
						id: 'operations',
						label: message('config.navigation.operations'),
						icon: 'shield',
					},
				],
			},
		],
	},
];

/** App-level entries rendered in the sidebar footer, outside any group. */
export const NAV_FOOTER: NavItem[] = [
	{
		href: '/settings',
		label: message('config.navigation.settings'),
		icon: 'sliders',
		description: message('config.navigation.settingsDescription'),
	},
];

export const NAV_ITEMS: NavItem[] = [...NAV_GROUPS.flatMap((group) => group.items), ...NAV_FOOTER];

export function findNavItem(href: string): NavItem | undefined {
	return NAV_ITEMS.find((item) => href === item.href || href.startsWith(`${item.href}/`));
}

export function navItemFor(pathname: string): NavItem | undefined {
	const match = NAV_ITEMS.filter(
		(item) => pathname === item.href || pathname.startsWith(`${item.href}/`),
	);
	return match.sort((a, b) => b.href.length - a.href.length)[0];
}
