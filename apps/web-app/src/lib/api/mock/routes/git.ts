/** Mock routes: the user-repo git workbench. */

import {
	gitMeta,
	gitFiles,
	gitDiff,
	gitStagedDiff,
	gitCommits,
	gitBranches,
	gitStash,
	gitReflog,
	gitRegistry,
} from '#lib/fixtures/git';
import type { MockRouter } from '../router';
import { changedCount, changedDiffs, changedFiles } from '../workspace';

export function registerGit(router: MockRouter): void {
	// ---------- user git ----------
	router.on('GET', '/api/v1/git/meta', () => gitMeta);
	router.on('GET', '/api/v1/git/status', () => ({
		branch: gitMeta.branch,
		upstream: gitMeta.upstream,
		ahead: gitMeta.ahead,
		behind: gitMeta.behind,
		staged: 2,
		unstaged: 1 + changedCount(),
		untracked: 1,
		conflicted: 0,
		merge_head: false,
		rebase_head: false,
		cherry_pick_head: false,
	}));
	router.on('GET', '/api/v1/git/files', () => [...gitFiles, ...changedFiles()]);
	router.on('GET', '/api/v1/git/diff', (_p, query) => {
		const path = typeof query.path === 'string' ? query.path.trim() : '';
		const all = [...gitDiff, ...changedDiffs()];
		if (!path) return all;
		const hit = all.filter((row) => row.path === path);
		return hit.length > 0 ? hit : all.slice(0, 1);
	});
	router.on('GET', '/api/v1/git/diff/staged', (_p, query) => {
		const path = typeof query.path === 'string' ? query.path.trim() : '';
		if (!path) return gitStagedDiff;
		return gitStagedDiff.filter((row) => row.path === path);
	});
	router.on('GET', '/api/v1/git/diff/range', (_p, query) => {
		const path = typeof query.path === 'string' ? query.path.trim() : '';
		const all = [...gitDiff, ...changedDiffs()];
		if (!path) return all;
		return all.filter((row) => row.path === path);
	});
	router.on('GET', '/api/v1/git/commits', () => gitCommits);
	router.on('GET', '/api/v1/git/commits/files', () => [
		{ path: gitFiles[0]?.path ?? 'preview.txt', status: 'M', renamed_from: null },
	]);
	router.on('GET', '/api/v1/git/branches', () => gitBranches);
	router.on('GET', '/api/v1/git/remotes', () => [
		{ name: 'origin', url: 'https://preview.example/wf-agent.git', branches: ['main'] },
	]);
	router.on('GET', '/api/v1/git/tags', () => [
		{ name: 'v1-preview', commit: 'previ3w', message: 'preview tag' },
	]);
	router.on('GET', '/api/v1/git/stash', () => gitStash);
	router.on('GET', '/api/v1/git/reflog', () => gitReflog);
	router.on('GET', '/api/v1/git/submodules', () => []);
	router.on('GET', '/api/v1/git/worktrees', () => [
		{ path: '/preview/wf-agent', branch: 'main', commit: 'previ3w', bare: false },
	]);
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
