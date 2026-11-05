/**
 * Shared in-memory workspace for the fixture-backed preview.
 *
 * One mutable source of truth for the preview filesystem. The fs routes,
 * the scripted PTY shell (`pty-socket.ts`) and the git routes all read and
 * write this state, so editing a file in the IDE is visible in the
 * terminal (`ls`, `cat`) and in the git status / diff fixtures — which is
 * what makes the preview able to exercise cross-panel UI/UX flows.
 *
 * Conflict simulation: files listed in `conflictPaths` report a rotating
 * hash on every read, so a normal save (which sends `expected_hash`)
 * always fails with a conflict error and the "Overwrite" flow can be
 * exercised; a forced save (no expected hash) succeeds.
 */

const INITIAL_DIRS: string[] = [
	'',
	'apps',
	'apps/web-app',
	'apps/web-app/src',
	'docs',
	'docs/plan',
	'crates',
	'src',
];

const INITIAL_FILES: Record<string, string> = {
	'README.md':
		'# wf-agent (preview)\n\nRead-only preview of the workspace filesystem.\nEdit freely — changes are visible to the IDE, the terminal and the git fixtures.\n',
	'apps/web-app/package.json': '{\n  "name": "@wf-agent/web-app-frontend"\n}\n',
	'apps/web-app/src/main.ts': '// preview entry\nconsole.log("preview workspace");\n',
	'docs/plan/lightweight-ide-design.md':
		'# Lightweight IDE (preview)\n\nFile editing in preview is mocked against the shared workspace state.\n',
	'crates/README.md': '# crates\n\nFoundation / infra / engine / app layers.\n',
	'src/conflict-demo.ts':
		'// Save this file to trigger the conflict flow, then use Overwrite.\nexport const demo = true;\n',
};

const CONFLICT_PATHS = new Set(['src/conflict-demo.ts']);

const FIXED_MTIME = 1_700_000_000_000;

/** Mutable set of directory paths ('' is the workspace root). */
const DIRS = new Set<string>(INITIAL_DIRS);

/** Mutable path → content map for regular files. */
const FILES = new Map<string, string>(Object.entries(INITIAL_FILES));

/** Snapshot of the pristine tree used to derive pseudo git changes. */
const BASELINE = new Map<string, string>(Object.entries(INITIAL_FILES));

/** Per-file content hashes; bumped on every write so saves round-trip. */
const HASHES = new Map<string, string>();
let hashSeq = 0;

function newHash(): string {
	hashSeq += 1;
	return `preview-hash-${hashSeq}`;
}

for (const path of FILES.keys()) HASHES.set(path, newHash());

function ensureDirs(path: string): void {
	const parts = path.split('/').filter(Boolean);
	let prefix = '';
	for (const part of parts) {
		prefix = prefix ? `${prefix}/${part}` : part;
		DIRS.add(prefix);
	}
}

export function hasConflictSimulation(path: string): boolean {
	return CONFLICT_PATHS.has(path);
}

/** Snapshot of all live file paths with their current content. */
export function allFiles(): Map<string, string> {
	return FILES;
}

export function hashFor(path: string): string {
	if (CONFLICT_PATHS.has(path)) return newHash();
	return HASHES.get(path) ?? newHash();
}

export interface FsEntryRow {
	name: string;
	path: string;
	is_dir: boolean;
	size: number;
	modified_ms: number;
}

/** List one directory, files and subdirectories sorted by name. */
export function listDir(path: string): FsEntryRow[] {
	const key = path.replace(/\/+$/, '');
	const prefix = key ? `${key}/` : '';
	const seen = new Set<string>();
	const rows: FsEntryRow[] = [];
	for (const dir of DIRS) {
		if (key === '' ? dir === '' : !dir.startsWith(prefix) || dir === key) continue;
		const rest = dir.slice(prefix.length);
		if (!rest || rest.includes('/')) continue;
		if (seen.has(rest)) continue;
		seen.add(rest);
		rows.push({ name: rest, path: dir, is_dir: true, size: 0, modified_ms: FIXED_MTIME });
	}
	for (const [filePath, content] of FILES) {
		if (key === '' ? filePath.includes('/') : !filePath.startsWith(prefix)) continue;
		const rest = filePath.slice(prefix.length);
		if (!rest || rest.includes('/')) continue;
		if (seen.has(rest)) continue;
		seen.add(rest);
		rows.push({ name: rest, path: filePath, is_dir: false, size: content.length, modified_ms: FIXED_MTIME });
	}
	rows.sort((a, b) => a.name.localeCompare(b.name));
	return rows;
}

/** Flatten the tree under `base`, depth-first, up to `limit` entries. */
export function flattenTree(base: string, limit: number): { entries: Array<{ path: string; is_dir: boolean; size: number }>; truncated: boolean } {
	const flat: Array<{ path: string; is_dir: boolean; size: number }> = [];
	let truncated = false;
	const walk = (prefix: string): boolean => {
		for (const row of listDir(prefix)) {
			if (flat.length >= limit) {
				truncated = true;
				return true;
			}
			flat.push({ path: row.path, is_dir: row.is_dir, size: row.size });
			if (row.is_dir && walk(row.path)) return true;
		}
		return false;
	};
	walk(base.replace(/\/+$/, ''));
	return { entries: flat, truncated };
}

export function statPath(path: string): { path: string; is_dir: boolean; size: number; modified_ms: number; is_binary: boolean | null; hash: string | null } {
	const key = path.replace(/\/+$/, '');
	if (key === '' || DIRS.has(key)) {
		return { path: key, is_dir: true, size: 0, modified_ms: FIXED_MTIME, is_binary: null, hash: null };
	}
	const content = FILES.get(key);
	if (content !== undefined) {
		return { path: key, is_dir: false, size: content.length, modified_ms: FIXED_MTIME, is_binary: false, hash: hashFor(key) };
	}
	return { path: key, is_dir: false, size: 128, modified_ms: FIXED_MTIME, is_binary: false, hash: null };
}

export function readFile(path: string): string {
	return FILES.get(path) ?? `# Preview file: ${path}\n\nMock content for the IDE preview.\n`;
}

export interface WriteResult {
	ok: true;
	path: string;
	content: string;
	hash: string;
}

export interface WriteConflict {
	ok: false;
	message: string;
}

/**
 * Write a file. When `expectedHash` is provided it must match the hash the
 * client last read; a mismatch resolves to a conflict so the UI's
 * "changed on disk" flow can be exercised.
 */
export function writeFile(path: string, content: string, expectedHash?: string | null): WriteResult | WriteConflict {
	if (expectedHash != null && expectedHash !== '' && HASHES.get(path) !== expectedHash) {
		return { ok: false, message: `${path} changed on disk (preview conflict): reload or overwrite.` };
	}
	FILES.set(path, content);
	HASHES.set(path, newHash());
	ensureDirs(path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '');
	return { ok: true, path, content, hash: HASHES.get(path) ?? newHash() };
}

export function createEntry(path: string, isDir: boolean): FsEntryRow {
	const key = path.replace(/\/+$/, '');
	if (isDir) {
		DIRS.add(key);
	} else {
		FILES.set(key, '');
		HASHES.set(key, newHash());
		ensureDirs(key.includes('/') ? key.slice(0, key.lastIndexOf('/')) : '');
	}
	const name = key.split('/').pop() ?? key;
	return { name, path: key, is_dir: isDir, size: 0, modified_ms: FIXED_MTIME };
}

/** Remove a file, or a directory with everything under it. */
export function removePath(path: string): boolean {
	const key = path.replace(/\/+$/, '');
	let removed = false;
	for (const filePath of [...FILES.keys()]) {
		if (filePath === key || filePath.startsWith(`${key}/`)) {
			FILES.delete(filePath);
			HASHES.delete(filePath);
			removed = true;
		}
	}
	for (const dir of [...DIRS]) {
		if (dir === key || dir.startsWith(`${key}/`)) {
			DIRS.delete(dir);
			removed = true;
		}
	}
	return removed;
}

export function movePath(from: string, to: string): FsEntryRow | null {
	const src = from.replace(/\/+$/, '');
	const dst = to.replace(/\/+$/, '');
	const content = FILES.get(src);
	if (content !== undefined) {
		removePath(dst);
		FILES.set(dst, content);
		HASHES.set(dst, HASHES.get(src) ?? newHash());
		removePath(src);
	} else if (DIRS.has(src)) {
		const under = [...DIRS].filter((d) => d === src || d.startsWith(`${src}/`));
		under.sort((a, b) => a.length - b.length);
		for (const dir of under) {
			DIRS.delete(dir);
			DIRS.add(dst + dir.slice(src.length));
		}
		for (const filePath of [...FILES.keys()]) {
			if (filePath.startsWith(`${src}/`)) {
				const next = dst + filePath.slice(src.length);
				FILES.set(next, FILES.get(filePath) ?? '');
				HASHES.set(next, HASHES.get(filePath) ?? newHash());
				FILES.delete(filePath);
				HASHES.delete(filePath);
			}
		}
	} else {
		return null;
	}
	ensureDirs(dst.includes('/') ? dst.slice(0, dst.lastIndexOf('/')) : '');
	const name = dst.split('/').pop() ?? dst;
	return { name, path: dst, is_dir: !FILES.has(dst), size: FILES.get(dst)?.length ?? 0, modified_ms: FIXED_MTIME };
}

export function copyPath(from: string, to: string): FsEntryRow | null {
	const src = from.replace(/\/+$/, '');
	const dst = to.replace(/\/+$/, '');
	const content = FILES.get(src);
	if (content === undefined) return null;
	FILES.set(dst, content);
	HASHES.set(dst, newHash());
	ensureDirs(dst.includes('/') ? dst.slice(0, dst.lastIndexOf('/')) : '');
	const name = dst.split('/').pop() ?? dst;
	return { name, path: dst, is_dir: false, size: content.length, modified_ms: FIXED_MTIME };
}

export function storeUpload(path: string, content: string): FsEntryRow {
	const write = writeFile(path, content, null);
	const row = statPath(write.ok ? write.path : path);
	const name = row.path.split('/').pop() ?? row.path;
	return { name, path: row.path, is_dir: false, size: row.size, modified_ms: FIXED_MTIME };
}

// ---------------------------------------------------------------------------
// Git linkage — pseudo status/diff derived from the baseline snapshot
// ---------------------------------------------------------------------------

export interface WorkspaceGitFile {
	path: string;
	staged: string;
	unstaged: string;
	kind: string;
	renamed_from: null;
}

export interface WorkspaceGitDiff {
	path: string;
	kind: string;
	hunks: Array<{ header: string; lines: string[] }>;
	additions: number;
	deletions: number;
	binary: boolean;
	truncated: boolean;
}

/** Diff one file against the baseline as pseudo unified hunks. */
function diffAgainstBaseline(path: string): WorkspaceGitDiff | null {
	const current = FILES.get(path);
	const before = BASELINE.get(path);
	if (current === before) return null;
	const kind = before === undefined ? 'added' : 'modified';
	const beforeLines = (before ?? '').split('\n');
	const afterLines = (current ?? '').split('\n');
	const lines: string[] = [];
	let additions = 0;
	let deletions = 0;
	const max = Math.max(beforeLines.length, afterLines.length);
	for (let i = 0; i < max; i += 1) {
		const oldLine = beforeLines[i];
		const newLine = afterLines[i];
		if (oldLine === newLine) {
			if (oldLine !== undefined) lines.push(` ${oldLine}`);
			continue;
		}
		if (oldLine !== undefined) {
			lines.push(`-${oldLine}`);
			deletions += 1;
		}
		if (newLine !== undefined) {
			lines.push(`+${newLine}`);
			additions += 1;
		}
	}
	return {
		path,
		kind,
		hunks: [{ header: `@@ -1,${beforeLines.length} +1,${afterLines.length} @@`, lines }],
		additions,
		deletions,
		binary: false,
		truncated: false,
	};
}

/** Pseudo `git files` rows for everything changed since the baseline. */
export function changedFiles(): WorkspaceGitFile[] {
	const rows: WorkspaceGitFile[] = [];
	for (const [path, content] of FILES) {
		if (BASELINE.get(path) === content) continue;
		rows.push({
			path,
			staged: '.',
			unstaged: BASELINE.has(path) ? 'M' : '?',
			kind: BASELINE.has(path) ? 'unstaged' : 'untracked',
			renamed_from: null,
		});
	}
	for (const path of BASELINE.keys()) {
		if (!FILES.has(path)) {
			rows.push({ path, staged: '.', unstaged: 'D', kind: 'unstaged', renamed_from: null });
		}
	}
	return rows;
}

/** Pseudo diffs for everything changed since the baseline. */
export function changedDiffs(): WorkspaceGitDiff[] {
	const diffs: WorkspaceGitDiff[] = [];
	for (const [path] of FILES) {
		const diff = diffAgainstBaseline(path);
		if (diff) diffs.push(diff);
	}
	return diffs;
}

/** Number of unstaged workspace changes, for the status counters. */
export function changedCount(): number {
	return changedFiles().length;
}
