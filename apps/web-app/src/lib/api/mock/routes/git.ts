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

/** Mutable preview git state so write actions visibly change the board.
 * The fixtures above are the pristine snapshot; these copies are what the
 * GET handlers serve and what POST /action mutates. */
let liveMeta: Omit<typeof gitMeta, 'upstream'> & { upstream: string | null } = {
	...gitMeta,
	upstream: gitMeta.upstream,
};
let liveFiles = gitFiles.map((file) => ({ ...file }));
const liveStash = gitStash.map((entry) => ({ ...entry }));
const liveBranches = gitBranches.map((branch) => ({ ...branch }));
const liveCommits = gitCommits.map((commit) => ({
	...commit,
	refs: [...commit.refs],
	parents: [...commit.parents],
}));
let mutationSeq = 0;

function reindexStash(): void {
	liveStash.forEach((entry, index) => {
		entry.index = index;
		entry.name = `stash@{${index}}`;
	});
}

function togglePreviewFile(path: string): string {
	const file = liveFiles.find((entry) => entry.path === path);
	if (!file) return `preview stage.toggle: ${path} is not on the board`;
	if (file.kind === 'conflicted') return `preview stage.toggle: resolve ${path} first`;
	if (file.kind === 'staged') {
		if (file.staged === 'A') {
			file.staged = '.';
			file.unstaged = '?';
			file.kind = 'untracked';
		} else {
			file.unstaged = file.staged;
			file.staged = '.';
			file.kind = 'unstaged';
		}
		mutationSeq += 1;
		return `preview unstaged ${path}`;
	}
	if (file.kind === 'untracked') {
		file.staged = 'A';
	} else {
		file.staged = file.unstaged === '.' ? 'M' : file.unstaged;
	}
	file.unstaged = '.';
	file.kind = 'staged';
	mutationSeq += 1;
	return `preview staged ${path}`;
}

function stageAllPreview(): string {
	let count = 0;
	for (const file of liveFiles) {
		if (file.kind === 'conflicted' || file.kind === 'staged') continue;
		if (file.kind === 'untracked') {
			file.staged = 'A';
		} else if (file.unstaged !== '.') {
			file.staged = file.unstaged;
		} else {
			continue;
		}
		file.unstaged = '.';
		file.kind = 'staged';
		count += 1;
	}
	mutationSeq += 1;
	return `preview staged ${count} file(s)`;
}

function unstageAllPreview(): string {
	let count = 0;
	for (const file of liveFiles) {
		if (file.kind !== 'staged') continue;
		if (file.staged === 'A') {
			file.staged = '.';
			file.unstaged = '?';
			file.kind = 'untracked';
		} else {
			file.unstaged = file.staged;
			file.staged = '.';
			file.kind = 'unstaged';
		}
		count += 1;
	}
	mutationSeq += 1;
	return `preview unstaged ${count} file(s)`;
}

function discardPreview(path: string): string {
	const at = liveFiles.findIndex((entry) => entry.path === path);
	if (at < 0) return `preview discard: ${path} is not on the board`;
	liveFiles.splice(at, 1);
	mutationSeq += 1;
	return `preview discarded ${path}`;
}

function commitPreview(message: string): string {
	const subject = message.trim() || 'preview commit';
	const id = `preview-commit-${liveCommits.length + 1}`;
	liveCommits.unshift({
		id,
		short_id: id.slice(0, 7),
		author: 'Preview Author',
		email: 'preview@example.com',
		timestamp: Math.floor(Date.now() / 1000),
		subject,
		body: '',
		parents: liveCommits.length > 0 && liveCommits[0] ? [liveCommits[0].id] : [],
		refs: [],
	});
	const committed = liveFiles.filter((file) => file.kind === 'staged').length;
	liveFiles = liveFiles.filter((file) => file.kind !== 'staged');
	mutationSeq += 1;
	return `preview committed ${committed} file(s): ${subject}`;
}

function stashPushPreview(message: string): string {
	const text = message.trim() || 'WIP on main: preview work';
	liveStash.unshift({ index: 0, name: 'stash@{0}', message: text });
	reindexStash();
	liveFiles = liveFiles.filter((file) => file.kind === 'conflicted');
	mutationSeq += 1;
	return `preview stashed: ${text}`;
}

function dropStashPreview(index: number, verb: string): string {
	const at = liveStash.findIndex((entry) => entry.index === index);
	if (at < 0) return `preview ${verb}: no stash@{${index}}`;
	const [removed] = liveStash.splice(at, 1);
	reindexStash();
	mutationSeq += 1;
	return `preview ${verb} ${removed?.name ?? `stash@{${index}}`}`;
}

function checkoutPreview(branch: string): string {
	const hit = liveBranches.find((entry) => entry.name === branch);
	if (!hit) return `preview checkout: no branch ${branch}`;
	for (const entry of liveBranches) entry.current = entry === hit;
	liveMeta = {
		...liveMeta,
		branch: hit.name,
		upstream: hit.upstream,
		ahead: hit.ahead,
		behind: hit.behind,
	};
	mutationSeq += 1;
	return `preview checked out ${branch}`;
}

function applyPreviewAction(action: string, params: Record<string, unknown>): string {
	switch (action) {
		case 'stage.toggle':
			return togglePreviewFile(String(params.path ?? ''));
		case 'stage.all':
			return stageAllPreview();
		case 'unstage.all':
			return unstageAllPreview();
		case 'discard':
			return discardPreview(String(params.path ?? ''));
		case 'commit':
		case 'commit.direct':
			return commitPreview(String(params.message ?? ''));
		case 'stash.push':
			return stashPushPreview(String(params.message ?? ''));
		case 'stash.pop':
			return dropStashPreview(Number(params.index ?? 0), 'popped');
		case 'stash.apply': {
			const index = Number(params.index ?? 0);
			const hit = liveStash.find((entry) => entry.index === index);
			return hit ? `preview applied ${hit.name}` : `preview apply: no stash@{${index}}`;
		}
		case 'stash.drop':
			return dropStashPreview(Number(params.index ?? 0), 'dropped');
		case 'branch.checkout':
			return checkoutPreview(String(params.branch ?? ''));
		case 'branch.create': {
			const name = String(params.branch ?? '').trim() || `preview-branch-${liveBranches.length}`;
			if (!liveBranches.some((entry) => entry.name === name)) {
				liveBranches.push({
					name,
					current: false,
					upstream: null,
					ahead: 0,
					behind: 0,
					commit: liveCommits[0]?.short_id ?? 'previ3w',
					subject: 'preview branch',
				});
				mutationSeq += 1;
			}
			return `preview created branch ${name}`;
		}
		case 'branch.delete': {
			const name = String(params.branch ?? '');
			const at = liveBranches.findIndex((entry) => entry.name === name);
			if (at < 0) return `preview delete: no branch ${name}`;
			liveBranches.splice(at, 1);
			mutationSeq += 1;
			return `preview deleted branch ${name}`;
		}
		default:
			return `preview ${action}`;
	}
}

export function registerGit(router: MockRouter): void {
	// ---------- user git ----------
	router.on('GET', '/api/v1/git/meta', () => liveMeta);
	router.on('GET', '/api/v1/git/status', () => ({
		branch: liveMeta.branch,
		upstream: liveMeta.upstream,
		ahead: liveMeta.ahead,
		behind: liveMeta.behind,
		staged: liveFiles.filter((file) => file.staged !== '.').length,
		unstaged:
			liveFiles.filter((file) => file.unstaged !== '.' && file.unstaged !== '?').length +
			changedCount(),
		untracked: liveFiles.filter((file) => file.kind === 'untracked').length,
		conflicted: liveFiles.filter((file) => file.kind === 'conflicted').length,
		merge_head: false,
		rebase_head: false,
		cherry_pick_head: false,
	}));
	router.on('GET', '/api/v1/git/files', () => [...liveFiles, ...changedFiles()]);
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
	router.on('GET', '/api/v1/git/commits', () => liveCommits);
	router.on('GET', '/api/v1/git/commits/files', () => [
		{ path: gitFiles[0]?.path ?? 'preview.txt', status: 'M', renamed_from: null },
	]);
	router.on('GET', '/api/v1/git/branches', () => liveBranches);
	router.on('GET', '/api/v1/git/remotes', () => [
		{ name: 'origin', url: 'https://preview.example/wf-agent.git', branches: ['main'] },
	]);
	router.on('GET', '/api/v1/git/tags', () => [
		{ name: 'v1-preview', commit: 'previ3w', message: 'preview tag' },
	]);
	router.on('GET', '/api/v1/git/stash', () => liveStash);
	router.on('GET', '/api/v1/git/reflog', () => gitReflog);
	router.on('GET', '/api/v1/git/submodules', () => []);
	router.on('GET', '/api/v1/git/worktrees', () => [
		{ path: '/preview/wf-agent', branch: 'main', commit: 'previ3w', bare: false },
	]);
	router.on('GET', '/api/v1/git/fingerprint', () => ({
		status_hash: `preview-status-${mutationSeq}`,
		head: liveMeta.head,
		branch_summary: 'preview-branches',
	}));
	router.on('GET', '/api/v1/git/registry', () => gitRegistry);
	router.on('POST', '/api/v1/git/action', (_p, _q, body) => {
		const action = String((body as { action?: unknown } | undefined)?.action ?? 'unknown');
		const params = ((body as { params?: unknown } | undefined)?.params ?? {}) as Record<
			string,
			unknown
		>;
		const message = applyPreviewAction(action, params);
		return { action, ok: true, message, echo: `git preview ${action}` };
	});
}
