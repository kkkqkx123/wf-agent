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
		id: 'overview',
		label: message('config.navigation.overview'),
		items: [
			{
				href: '/dashboard',
				label: message('config.navigation.dashboard'),
				icon: 'gauge',
				description: message('config.navigation.dashboardDescription'),
			},
		],
	},
	{
		id: 'conversation',
		label: message('config.navigation.conversation'),
		items: [
			{
				href: '/chat',
				label: message('config.navigation.chat'),
				icon: 'sparkles',
				description: message('config.navigation.chatDescription'),
			},
		],
	},
	{
		id: 'execution',
		label: message('config.navigation.execution'),
		items: [
			{
				href: '/executions',
				label: message('config.navigation.workbench'),
				icon: 'activity',
				description: message('config.navigation.workbenchDescription'),
			},
		],
	},
	{
		id: 'orchestration',
		label: message('config.navigation.orchestration'),
		items: [
			{
				href: '/workflows',
				label: message('config.navigation.workflows'),
				icon: 'workflow',
				description: message('config.navigation.workflowsDescription'),
			},
			{
				href: '/templates',
				label: message('config.navigation.templates'),
				icon: 'template',
				description: message('config.navigation.templatesDescription'),
			},
		],
	},
	{
		id: 'state',
		label: message('config.navigation.state'),
		items: [
			{
				href: '/checkpoints',
				label: message('config.navigation.checkpoints'),
				icon: 'archive',
				description: message('config.navigation.checkpointsDescription'),
			},
			{
				href: '/git',
				label: message('config.navigation.git'),
				icon: 'git-commit',
				description: message('config.navigation.gitDescription'),
			},
		],
	},
	{
		id: 'integration',
		label: message('config.navigation.integration'),
		items: [
			{
				href: '/triggers',
				label: message('config.navigation.triggers'),
				icon: 'zap',
				description: message('config.navigation.triggersDescription'),
			},
			{
				href: '/resources',
				label: message('config.navigation.modelsTools'),
				icon: 'cpu',
				description: message('config.navigation.modelsToolsDescription'),
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
		id: 'insight',
		label: message('config.navigation.insight'),
		items: [
			{
				href: '/insights',
				label: message('config.navigation.queryAudit'),
				icon: 'chart',
				description: message('config.navigation.queryAuditDescription'),
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
				label: message('config.navigation.eventsSystem'),
				icon: 'radio',
				description: message('config.navigation.eventsSystemDescription'),
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
	{
		id: 'workspace',
		label: message('config.navigation.workspace'),
		items: [
			{
				href: '/settings',
				label: message('config.navigation.settings'),
				icon: 'sliders',
				description: message('config.navigation.settingsDescription'),
			},
		],
	},
];

export const NAV_ITEMS: NavItem[] = NAV_GROUPS.flatMap((group) => group.items);

export function findNavItem(href: string): NavItem | undefined {
	return NAV_ITEMS.find((item) => href === item.href || href.startsWith(`${item.href}/`));
}

export function navItemFor(pathname: string): NavItem | undefined {
	const match = NAV_ITEMS.filter(
		(item) => pathname === item.href || pathname.startsWith(`${item.href}/`),
	);
	return match.sort((a, b) => b.href.length - a.href.length)[0];
}
