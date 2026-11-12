/**
 * Shared in-memory workspace for the fixture-backed preview.
 *
 * One mutable source of truth for the preview filesystem. The fs routes,
 * the scripted PTY shell (`pty-socket.ts`) and the git routes all read and
 * write this state, so editing a file in the IDE is visible in the
 * terminal (`ls`, `cat`) and in the git status / diff fixtures — which is
 * what makes the preview able to exercise cross-panel UI/UX flows.
 *
 * The tree below is intentionally representative of a real checkout:
 * nested app / crates / docs / assets / configs / scripts areas, several
 * languages (ts, svelte, rs, md, json, yaml, toml, sh, py, sql, css),
 * a binary asset, a large truncated file and an empty directory. That gives
 * the file tree, quick-open, content search, editor, terminal and history
 * panels enough shape to verify UI/UX in preview mode without a backend.
 *
 * Error behavior mirrors the real backend on purpose: unknown paths resolve
 * to 404 (via `fileView` / `statPath` returning null), never to invented
 * filler content, so a stale tree row or ghost tab surfaces as an error
 * state in preview exactly like it would against `wf-server`.
 *
 * Conflict simulation: files listed in `conflictPaths` report a rotating
 * hash on every read, so a normal save (which sends `expected_hash`)
 * always fails with a conflict error and the "Overwrite" flow can be
 * exercised; a forced save (no expected hash) succeeds.
 */

// Only hand-declared directories live here: every ancestor of a file below is
// derived automatically (see the ensureDirs pass under INITIAL_FILES), so this
// list stays small and cannot drift from the file set.
const INITIAL_DIRS: string[] = [
	'',
	// Empty directory on purpose: exercises the empty tree / drop-target UX.
	'scratch/empty',
];

const INITIAL_FILES: Record<string, string> = {
	'README.md':
		'# wf-agent (preview)\n\nPreview workspace for IDE UI/UX checks.\nEdit freely — changes are visible to the IDE, the terminal and the git fixtures.\n\n' +
		'- `apps/web-app` — frontend routes, stores and services\n' +
		'- `crates` — Rust workspace (app / foundation layers)\n' +
		'- `crates/checkpoint` — restore coordinator + probe sources referenced by checkpoint fixtures\n' +
		'- `docs` — plans and architecture notes\n' +
		'- `scratch` — playground for create / rename / upload flows\n' +
		'- `src/big.ts` — large truncated file demo\n' +
		'- `assets/logo.png` — binary file demo\n' +
		'- `scratch/empty` — empty directory demo\n',
	'Cargo.toml':
		'[workspace]\nresolver = "2"\nmembers = ["crates/app/wf-api", "crates/app/wf-server", "crates/foundation/wf-core"]\n\n# preview manifest for the IDE tree demo\n',
	'.gitignore': 'target/\nnode_modules/\ndist/\n*.local\n.preview-cache/\n',
	'.editorconfig': 'root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\nindent_size = 2\n',
	'apps/web-app/package.json':
		'{\n  "name": "@wf-agent/web-app-frontend",\n  "private": true,\n  "scripts": {\n    "dev:preview": "vite dev --mode preview",\n    "check:preview": "svelte-check --tsconfig ./tsconfig.preview.json"\n  }\n}\n',
	'apps/web-app/svelte.config.js':
		'// preview svelte config\n// TODO: verify HMR stays fast with the preview workspace mounted.\nexport const config = { preview: true };\n',
	'apps/web-app/src/main.ts':
		'// preview entry — open this file to check tabs, dirty dot and save.\n// The preview workspace shares state with the terminal: try `cat apps/web-app/src/main.ts`.\nconsole.log("preview workspace");\n\nexport const previewRoot = "/preview/wf-agent";\n',
	'apps/web-app/src/app.css':
		'/* preview stylesheet — check the CSS language label in the editor status bar */\n.preview-shell {\n  background: var(--card);\n  color: var(--foreground);\n}\n',
	'apps/web-app/src/routes/ide/+page.svelte':
		'<!-- preview IDE route — search "page" in quick-open to find the three routes -->\n<script lang="ts">\n  // TODO: confirm the responsive sidebar overlay on narrow viewports.\n  const previewHint = "ide tree + editor + terminal";\n</script>\n\n<main>{previewHint}</main>\n',
	'apps/web-app/src/routes/git/+page.svelte':
		'<!-- preview git route — has an unstaged git badge and a workspace diff -->\n<script lang="ts">\n  // git workbench preview: staged, unstaged and conflicted rows drive the tree badges.\n  const previewBranch = "main";\n</script>\n\n<main>git workbench ({previewBranch})</main>\n',
	'apps/web-app/src/routes/checkpoints/+page.svelte':
		'<!-- third route so the routes folder shows multiple children -->\n<script lang="ts">\n  const previewTitle = "checkpoints";\n</script>\n\n<main>{previewTitle}</main>\n',
	'apps/web-app/src/lib/stores/ide.svelte.ts':
		'// preview IDE store — open with content search for "IdeStore".\n// TODO: verify tab eviction with many open files.\nexport interface IdePreviewTab {\n  path: string;\n  preview: boolean;\n}\n\nexport const IDE_PREVIEW_LIMIT = 20;\n',
	'apps/web-app/src/lib/services/fs.ts':
		'// preview fs service — search "workspace" to see this file grouped with others.\nexport function languageForPreview(path: string): string {\n  if (path.endsWith(".svelte")) return "svelte";\n  return "plaintext";\n}\n',
	'apps/web-app/src/lib/services/git.ts':
		'// preview git service — badges in the file tree derive from git status.\n// Try editing this file, then check the history panel changes section.\nexport const PREVIEW_GIT_ROOT = "/preview/wf-agent";\n',
	'apps/web-app/src/lib/utils/ide-url.ts':
		'// preview ide-url helper — quick-open "ide-u" should rank this first.\nexport function buildPreviewIdeUrl(path: string): string {\n  return `/ide?path=${encodeURIComponent(path)}`;\n}\n',
	'apps/web-app/src/lib/utils/mentions.ts':
		'// preview mentions helper — quick-open "men" should find this file.\nexport function parsePreviewMention(text: string): string | null {\n  return text.startsWith("@") ? text : null;\n}\n',
	'apps/web-app/src/lib/components/domain/ide/FileTree.svelte':
		'<!-- deep nesting demo: components/domain/ide/FileTree.svelte -->\n<script lang="ts">\n  // Hover a row to reveal row actions; right-click for the context menu.\n  const previewRowHeight = 28;\n</script>\n\n<div>file tree ({previewRowHeight}px rows)</div>\n',
	'apps/web-app/src/lib/components/domain/ide/FileEditor.svelte':
		'<!-- deep nesting demo: components/domain/ide/FileEditor.svelte -->\n<script lang="ts">\n  // Dirty tabs show a dot; the status bar shows language + cursor.\n  const previewLanguages = ["typescript", "svelte", "rust", "markdown"];\n</script>\n\n<div>file editor</div>\n',
	'crates/README.md':
		'# crates\n\nFoundation / infra / engine / app layers.\n\nPreview: open a `.rs` file to check the Rust language label.\n',
	'crates/checkpoint/src/restore_coordinator.rs':
		'//! preview restore coordinator — mirrors the checkpoint diff fixtures.\n//!\n//! Idempotent restore: the branch existence probe runs before the snapshot\n//! write, so repeated restores converge instead of raising a conflict.\nuse crate::preview::Workspace;\n\n// restore_coordinator.rs:141 — branch_exists returns Err(BranchConflict)\n// when the probe runs after the write; keep the probe first.\npub fn restore(workspace: &Workspace, target: &str) -> Result<String, String> {\n    if let Some(existing) = workspace.find_snapshot(target) {\n        return Ok(existing);\n    }\n    if workspace.branch_exists(target) {\n        return Err(format!("BranchConflict({target})"));\n    }\n    workspace.write_snapshot(target)\n}\n',
	'crates/checkpoint/src/branch_probe.rs':
		'//! preview branch probe — the agent loop summary traces the conflict origin here.\n//!\n//! Search "branch_probe" to jump between the loop transcript and this source.\npub struct Probe {\n    branch: String,\n    use_cache: bool,\n}\n\nimpl Probe {\n    pub fn new(branch: &str) -> Self {\n        Self { branch: branch.to_string(), use_cache: false }\n    }\n\n    pub fn with_cache(mut self, use_cache: bool) -> Self {\n        self.use_cache = use_cache;\n        self\n    }\n\n    pub fn run(&self) -> bool {\n        // preview workspace probe: cached reads keep the tree badges fresh.\n        !self.branch.is_empty()\n    }\n}\n',
	'crates/checkpoint/src/idempotent_restore.rs':
		'//! preview idempotent restore — added-file demo from the staged diff.\npub fn restore_once(target: &str) -> Result<String, String> {\n    // Converges when the branch already holds a snapshot for target.\n    Ok(format!("preview-snapshot:{target}"))\n}\n',
	'crates/checkpoint/src/legacy_rollback.rs':
		'//! preview legacy rollback — kept so the files panel shows a second writer.\n// TODO: remove once the coordinator covers every rollback path.\npub fn rollback_to(target: &str, snapshot: &str) -> String {\n    format!("preview-rollback:{target}:{snapshot}")\n}\n',
	'crates/checkpoint/tests/restore_repeat.rs':
		'// preview repeat-restore test — restore twice, converge once.\n// Mirrors docs/plan/checkpoint-restore.md acceptance checks.\n#[test]\nfn restore_twice_converges() {\n    let workspace = crate::preview::Workspace::default();\n    let first = crate::restore_once("preview-branch");\n    assert!(first.is_ok());\n    assert!(workspace.find_snapshot("preview-branch").is_some());\n}\n',
	'crates/app/wf-api/src/lib.rs':
		'//! preview api facade\n//!\n//! Search "flattenTree" to jump between the Rust and TypeScript sides.\npub mod git;\n\npublic fn preview_name() -> &\'static str {\n    "wf-api-preview"\n}\n',
	'crates/app/wf-api/src/git/queries.rs':
		'//! preview git queries (staged modification demo)\n//!\n//! Edit this file in the IDE and watch the history panel changes grow.\nuse crate::preview::Workspace;\n\npub fn staged_query_count() -> usize {\n    2 // TODO: cover pagination in preview\n}\n',
	'crates/app/wf-server/src/main.rs':
		'// preview server entry\n// The preview never talks to this binary; it only feeds the tree/search demo.\nfn main() {\n    println!("preview wf-server");\n}\n',
	'crates/app/wf-server/src/api/git/readonly.rs':
		'//! preview read-only git api (staged addition demo)\nuse crate::preview::Workspace;\n\npub fn readonly_paths() -> Vec<&\'static str> {\n    vec!["apps/web-app/src/routes/git/+page.svelte", "src/big.ts"]\n}\n',
	'crates/foundation/wf-core/src/lib.rs':
		'//! preview core library\n//!\n//! Small Rust file for language + gutter-mark checks.\npub fn preview_version() -> &\'static str {\n    "1.0.0-preview"\n}\n',
	'crates/foundation/wf-core/src/exec.rs':
		'//! preview exec engine\n// TODO: surface cancellation state in the IDE status bar.\npub enum PreviewStep {\n    Read,\n    Write,\n}\n',
	'docs/architecture.md':
		'# Architecture (preview)\n\nFour layers: foundation / infra / engine / app.\nThe preview workspace mirrors that shape so the tree reads like the real repo.\n\nSearch "workspace" to see cross-layer hits grouped by file.\n',
	'docs/plan/lightweight-ide-design.md':
		'# Lightweight IDE (preview)\n\nFile editing in preview is mocked against the shared workspace state.\nOpen `docs/plan/ide/preview-checklist.md` for the UI/UX walkthrough.\n',
	'docs/plan/checkpoint-restore.md':
		'# Checkpoint restore (preview)\n\nPlan backing the `crates/checkpoint` sources in this workspace.\nOpen the matching `.rs` files to verify diffs, badges and history together.\n\n- [ ] probe before write in `restore_coordinator.rs`\n- [ ] cached probe in `branch_probe.rs`\n- [ ] TODO: repeat-restore test in `tests/restore_repeat.rs`\n- [ ] TODO: verify rollback note in `legacy_rollback.rs`\n',
	'docs/plan/user-git/05-progress.md':
		'# User git progress (preview, untracked)\n\nThis file is untracked on purpose so the tree shows an untracked badge.\n\n- [ ] verify empty directory UX in `scratch/empty`\n- [ ] TODO: confirm binary view for `assets/logo.png`\n- [ ] TODO: confirm truncated banner for `src/big.ts`\n',
	'docs/plan/ide/preview-checklist.md':
		'# IDE preview checklist\n\nWalk the UI/UX without a backend:\n\n1. Tree: expand `apps`, `crates`, `scratch`; check badges and hover actions.\n2. Quick-open: search `restore`, `ide`, `page`.\n3. Content search: search `preview`, `TODO`, `workspace`; try scope `crates` and include `*.rs`.\n4. Editor: open `src/big.ts` (truncated), `assets/logo.png` (binary), `src/conflicted.ts` (conflict).\n5. History: changes + commits + diff dialog.\n6. Terminal: `ls`, `ls scratch`, `cat README.md`.\n7. Markdown: open `scratch/README.md`, switch source / preview / split and drag the splitter; the file icon resolves from the workspace.\n',
	'src/conflict-demo.ts':
		'// Save this file to trigger the conflict flow, then use Overwrite.\nexport const demo = true;\n',
	'src/conflicted.ts':
		'// preview conflicted file — matches the git conflicted badge + diff fixture.\n<<<<<<< HEAD\n// TODO: keep the preview toolbar pinned while resolving.\nexport const ours = 1;\n=======\nexport const theirs = 2;\n>>>>>>> preview/demo\n',
	'assets/logo.png': 'PNG preview placeholder — binary demo (editor shows the binary view).\n',
	'assets/icons/file.svg':
		'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><!-- preview file icon --><rect width="16" height="16" /></svg>\n',
	'assets/icons/folder.svg':
		'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><!-- preview folder icon --><rect width="16" height="16" /></svg>\n',
	'configs/preview.yaml':
		'# preview config — checks the YAML language label\nworkspace: /preview/wf-agent\ntheme: preview\nfeatures:\n  - tree\n  - search\n  - terminal\n',
	'configs/app.json':
		'{\n  "workspace": "/preview/wf-agent",\n  "preview": true,\n  "limits": { "tabs": 20, "search": 100 }\n}\n',
	'configs/checkpoint.toml':
		'# preview checkpoint store — checks the TOML language label\n[workspace]\nroot = "/preview/wf-agent"\n\n[preview.checkpoint]\nbackend = "sqlite"\npartition = "approval"\n',
	'configs/release.toml':
		'# preview release inputs — referenced by the executions and triggers fixtures\n[workspace]\nroot = "/preview/wf-agent"\n\n[preview.release]\nchannel = "preview"\nversion = "1.0.0"\n',
	'scripts/preview.sh':
		'#!/bin/sh\n# preview helper — `cat scripts/preview.sh` in the terminal should match the editor.\n# TODO: add a preview smoke check for the empty directory.\necho "preview workspace: $PWD"\nls scratch\n',
	'scripts/check-preview.mjs':
		'// preview node script — JavaScript language demo\n// Search "preview" groups this with the shell and python helpers.\nconsole.log("preview check: tree + search + terminal");\n',
	'scripts/checkpoint-verify.sh':
		'#!/bin/sh\n# preview checkpoint verify — matches the scripted tool name in fixtures.\n# `cat scripts/checkpoint-verify.sh` in the terminal should match the editor.\n# TODO: fail loudly when the restore snapshot is missing.\necho "preview verify: $PWD"\nls crates/checkpoint/src\n',
	'pytools/analyze.py':
		'# preview python helper — Python language demo\n# TODO: report preview search hit counts per directory.\ndef preview_summary(workspace):\n    return f"preview workspace: {workspace}"\n',
	'queries/report.sql':
		'-- preview sql — SQL language demo\n-- Search "workspace" to group this with the yaml/json configs.\nSELECT path, COUNT(*) AS preview_hits FROM workspace_files GROUP BY path;\n',
	'scratch/README.md':
		'# scratch (preview)\n\nPlayground for create / rename / upload flows. `scratch/empty` is\nintentionally empty to demo the empty tree state and file drop targets.\n\n![file icon](../assets/icons/file.svg)\n',
};

// Large file: exercises the truncated banner, read-only Monaco state and
// long-file scrolling. Every fifth row mentions preview so content search
// for "preview" reliably overflows the default limit and shows load-more.
{
	const lines: string[] = [
		'// src/big.ts — large truncated file demo (read-only banner in the editor).',
		'// Open this file to verify the truncated notice and the git unstaged badge.',
	];
	for (let i = 1; i <= 350; i += 1) {
		if (i % 5 === 0) {
			lines.push(`// preview workspace marker ${i} — keeps content search pagination honest.`);
		} else {
			lines.push(`export const bigRow${i} = ${i};`);
		}
	}
	INITIAL_FILES['src/big.ts'] = `${lines.join('\n')}\n`;
}

const CONFLICT_PATHS = new Set(['src/conflict-demo.ts']);

const FIXED_MTIME = 1_700_000_000_000;

/** Extensions served as binary in preview (editor binary view + git binary diff). */
const BINARY_EXTS = [
	'.png',
	'.jpg',
	'.jpeg',
	'.gif',
	'.ico',
	'.pdf',
	'.woff',
	'.woff2',
	'.zip',
	'.bin',
];

/** Paths that always preview as truncated, plus a size fallback below. */
const ALWAYS_TRUNCATED = new Set(['src/big.ts']);
const TRUNCATED_SIZE_THRESHOLD = 8000;
const TRUNCATED_LINE_LIMIT = 200;

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

// Defensive: every initial file implies its ancestor directories, so the
// tree can never silently drop a folder when the curated lists drift.
for (const path of Object.keys(INITIAL_FILES)) {
	const slash = path.lastIndexOf('/');
	if (slash > 0) ensureDirs(path.slice(0, slash));
}

export function hasConflictSimulation(path: string): boolean {
	return CONFLICT_PATHS.has(path);
}

/** Snapshot of all live file paths with their current content. */
export function allFiles(): Map<string, string> {
	return FILES;
}

/** Binary preview is extension-based so images and fonts hit the binary UX. */
export function isBinaryPath(path: string): boolean {
	const lower = path.toLowerCase();
	return BINARY_EXTS.some((ext) => lower.endsWith(ext));
}

function isTruncatedPath(path: string, size: number): boolean {
	return ALWAYS_TRUNCATED.has(path) || size > TRUNCATED_SIZE_THRESHOLD;
}

/** First N lines of a large file, matching the truncated editor contract. */
function truncateLines(content: string): string {
	const lines = content.split('\n');
	if (lines.length <= TRUNCATED_LINE_LIMIT) return content;
	return `${lines.slice(0, TRUNCATED_LINE_LIMIT).join('\n')}\n… (truncated preview of ${lines.length} lines)\n`;
}

export interface FileView {
	content: string;
	size: number;
	truncated: boolean;
	isBinary: boolean;
	hash: string;
}

/**
 * Editor-facing view of one file: binary flag, truncation and hash.
 * Unknown paths resolve to null (the routes turn this into a 404, like the
 * real backend) — the preview never invents content for paths that do not
 * exist, so stale tree rows and ghost tabs surface as error states instead
 * of plausible-looking filler.
 */
export function fileView(path: string): FileView | null {
	const key = path.replace(/\/+$/, '');
	const raw = FILES.get(key);
	if (raw === undefined) return null;
	const content = raw;
	const size = raw.length;
	const isBinary = isBinaryPath(key);
	if (isBinary) {
		return { content, size, truncated: false, isBinary: true, hash: hashFor(key) };
	}
	if (isTruncatedPath(key, size)) {
		return {
			content: truncateLines(content),
			size,
			truncated: true,
			isBinary: false,
			hash: hashFor(key),
		};
	}
	return { content, size, truncated: false, isBinary: false, hash: hashFor(key) };
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
		rows.push({
			name: rest,
			path: filePath,
			is_dir: false,
			size: content.length,
			modified_ms: FIXED_MTIME,
		});
	}
	rows.sort((a, b) => a.name.localeCompare(b.name));
	return rows;
}

/** Flatten the tree under `base`, depth-first, up to `limit` entries. */
export function flattenTree(
	base: string,
	limit: number,
): { entries: Array<{ path: string; is_dir: boolean; size: number }>; truncated: boolean } {
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

/**
 * Metadata for one path. Unknown paths resolve to null (404 at the route),
 * matching the real backend: callers that probe best-effort (external-change
 * detection) already tolerate the failure.
 */
export function statPath(path: string): {
	path: string;
	is_dir: boolean;
	size: number;
	modified_ms: number;
	is_binary: boolean | null;
	hash: string | null;
} | null {
	const key = path.replace(/\/+$/, '');
	if (key === '' || DIRS.has(key)) {
		return {
			path: key,
			is_dir: true,
			size: 0,
			modified_ms: FIXED_MTIME,
			is_binary: null,
			hash: null,
		};
	}
	const content = FILES.get(key);
	if (content !== undefined) {
		return {
			path: key,
			is_dir: false,
			size: content.length,
			modified_ms: FIXED_MTIME,
			is_binary: isBinaryPath(key) ? true : false,
			hash: hashFor(key),
		};
	}
	return null;
}

/** True when `path` names the workspace root, a known directory or a file. */
export function hasPath(path: string): boolean {
	const key = path.replace(/\/+$/, '');
	return key === '' || DIRS.has(key) || FILES.has(key);
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
export function writeFile(
	path: string,
	content: string,
	expectedHash?: string | null,
): WriteResult | WriteConflict {
	if (expectedHash != null && expectedHash !== '' && HASHES.get(path) !== expectedHash) {
		return {
			ok: false,
			message: `${path} changed on disk (preview conflict): reload or overwrite.`,
		};
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
	return {
		name,
		path: dst,
		is_dir: !FILES.has(dst),
		size: FILES.get(dst)?.length ?? 0,
		modified_ms: FIXED_MTIME,
	};
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
	if (!write.ok) {
		// Unreachable: a null expected hash never conflicts. Throw instead of
		// inventing a fallback row so a real regression fails loudly.
		throw new Error(`Preview upload failed for ${path}: ${write.message}`);
	}
	const name = write.path.split('/').pop() ?? write.path;
	return {
		name,
		path: write.path,
		is_dir: false,
		size: write.content.length,
		modified_ms: FIXED_MTIME,
	};
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
	if (isBinaryPath(path)) {
		return {
			path,
			kind: 'modified',
			hunks: [],
			additions: 0,
			deletions: 0,
			binary: true,
			truncated: false,
		};
	}
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
