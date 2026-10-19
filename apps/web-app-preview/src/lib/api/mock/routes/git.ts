/** Mock routes: the user-repo git workbench. */

import {
	gitMeta,
	gitFiles,
	gitDiff,
	gitCommits,
	gitBranches,
	gitStash,
	gitRegistry,
} from '$lib/fixtures/git';
import type { MockRouter } from '../router';

export function registerGit(router: MockRouter): void {
	// ---------- user git ----------
	router.on('GET', '/api/v1/git/meta', () => gitMeta);
	router.on('GET', '/api/v1/git/status', () => ({
		branch: gitMeta.branch,
		upstream: gitMeta.upstream,
		ahead: gitMeta.ahead,
		behind: gitMeta.behind,
		staged: 2,
		unstaged: 1,
		untracked: 1,
		conflicted: 0,
		merge_head: false,
		rebase_head: false,
		cherry_pick_head: false,
	}));
	router.on('GET', '/api/v1/git/files', () => gitFiles);
	router.on('GET', '/api/v1/git/diff', () => gitDiff);
	router.on('GET', '/api/v1/git/diff/range', () => gitDiff);
	router.on('GET', '/api/v1/git/commits', () => gitCommits);
	router.on('GET', '/api/v1/git/commits/files', () => ([
		{ path: gitFiles[0]?.path ?? 'preview.txt', status: 'M', renamed_from: null },
	]));
	router.on('GET', '/api/v1/git/branches', () => gitBranches);
	router.on('GET', '/api/v1/git/remotes', () => ([
		{ name: 'origin', url: 'https://preview.example/wf-agent.git', branches: ['main'] },
	]));
	router.on('GET', '/api/v1/git/tags', () => ([
		{ name: 'v1-preview', commit: 'previ3w', message: 'preview tag' },
	]));
	router.on('GET', '/api/v1/git/stash', () => gitStash);
	router.on('GET', '/api/v1/git/submodules', () => ([]));
	router.on('GET', '/api/v1/git/worktrees', () => ([
		{ path: '/preview/wf-agent', branch: 'main', commit: 'previ3w', bare: false },
	]));
	router.on('GET', '/api/v1/git/fingerprint', () => ({
		status_hash: 'preview-status',
		head: gitMeta.head,
		branch_summary: 'preview-branches',
	}));
	router.on('GET', '/api/v1/git/registry', () => gitRegistry);
	router.on('POST', '/api/v1/git/action', (_p, _q, body) => {
		const action = (body as { action?: string } | undefined)?.action ?? 'unknown';
		return { action, ok: true, message: `preview ${action}`, echo: `git preview ${action}` };
	});
}
