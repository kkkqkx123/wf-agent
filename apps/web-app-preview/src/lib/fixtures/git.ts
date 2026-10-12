export const gitMeta = {
	name: 'wf-agent',
	root: '/preview/wf-agent',
	branch: 'main',
	upstream: 'origin/main',
	ahead: 2,
	behind: 0,
	head: 'preview-head-abc1234',
	bare: false,
	merge_head: false,
	rebase_head: false,
	cherry_pick_head: false,
};

export const gitFiles = [
	{ path: 'crates/app/wf-api/src/git/queries.rs', staged: 'M', unstaged: '.', kind: 'staged', renamed_from: null },
	{ path: 'crates/app/wf-server/src/api/git/readonly.rs', staged: 'A', unstaged: '.', kind: 'staged', renamed_from: null },
	{ path: 'apps/web-app/src/routes/git/+page.svelte', staged: '.', unstaged: 'M', kind: 'unstaged', renamed_from: null },
	{ path: 'docs/plan/user-git/05-progress.md', staged: '?', unstaged: '?', kind: 'untracked', renamed_from: null },
];

export const gitDiff = [
	{
		path: 'apps/web-app/src/routes/git/+page.svelte',
		hunks: [
			{
				header: '@@ -1,3 +1,4 @@',
				lines: [' context', '-old', '+new', '+extra', ' context'],
			},
		],
		additions: 2,
		deletions: 1,
	},
];

export const gitCommits = [
	{
		id: 'preview-commit-3',
		short_id: 'previ3w',
		author: 'Preview Author',
		email: 'preview@example.com',
		timestamp: 1_700_000_300,
		subject: 'Add git workbench skeleton',
		body: '',
		parents: ['preview-commit-2'],
		refs: ['HEAD -> main'],
	},
	{
		id: 'preview-commit-2',
		short_id: 'previ2w',
		author: 'Preview Author',
		email: 'preview@example.com',
		timestamp: 1_700_000_200,
		subject: 'Add user git readonly domain',
		body: '',
		parents: ['preview-commit-1'],
		refs: [],
	},
];

export const gitBranches = [
	{ name: 'main', current: true, upstream: 'origin/main', ahead: 2, behind: 0, commit: 'previ3w', subject: 'Add git workbench skeleton' },
	{ name: 'preview/demo', current: false, upstream: null, ahead: 0, behind: 1, commit: 'previ2w', subject: 'Add user git readonly domain' },
];

export const gitStash = [
	{ index: 0, name: 'stash@{0}', message: 'WIP on main: preview work' },
];

export const gitRegistry = [
	{ id: 'refresh', danger: 'readonly', params: [], available: true, reason: null },
	{ id: 'stage.toggle', danger: 'recoverable', params: ['path'], available: true, reason: null },
	{ id: 'commit', danger: 'recoverable', params: ['message'], available: true, reason: null },
	{ id: 'branch.delete', danger: 'destructive', params: ['branch'], available: true, reason: null },
	{ id: 'push', danger: 'history_rewrite', params: ['remote', 'branch'], available: true, reason: null },
	{ id: 'workspace.write', danger: 'destructive', params: [], available: false, reason: 'worktree writes are unavailable' },
];
