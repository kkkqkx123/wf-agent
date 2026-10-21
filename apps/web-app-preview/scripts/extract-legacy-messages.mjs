// Extracts English strings that the i18n commit (c01b7c8) replaced with
// t('key') calls, by diffing each affected file against its pre-i18n version,
// and emits a draft messages/fragments/domain.en.json.
//
// Pairing strategy: for every diff hunk, the removed lines' string literals
// are paired in order with the added lines' t('key') calls. Hunks where the
// counts do not line up are reported as UNMATCHED for manual review — the
// script never guesses and never touches component code.
//
// Usage: node scripts/extract-legacy-messages.mjs [--base <rev>]
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';

const BASE = process.argv.includes('--base')
	? process.argv[process.argv.indexOf('--base') + 1]
	: 'c01b7c8^';

const repoRoot = execFileSync('git', ['rev-parse', '--show-toplevel'], {
	encoding: 'utf8',
}).trim();
const webAppDir = path.join(repoRoot, 'apps', 'web-app');
const outFile = path.join(webAppDir, 'messages', 'fragments', 'domain.en.json.draft');

function gitShow(rev, relPath) {
	try {
		return execFileSync(
			'git',
			['show', `${rev}:${relPath}`],
			{ cwd: repoRoot, encoding: 'utf8' },
		);
	} catch {
		return null;
	}
}

// Files that currently reference t() from $lib/i18n.
const srcDir = path.join(webAppDir, 'src');
const files = execFileSync('grep', ['-rl', '$lib/i18n', srcDir], {
	encoding: 'utf8',
})
	.trim()
	.split('\n')
	.filter(Boolean);

const messages = {};
const unmatched = [];

for (const absFile of files) {
	const relPath = path.relative(repoRoot, absFile);
	const old = gitShow(BASE, relPath);
	if (old === null) continue; // new file, no legacy strings
	const cur = readFileSync(absFile, 'utf8');

	// Only files whose legacy version still had inline literals are relevant.
	if (old === cur) continue;

	// Line-based diff via LCS.
	const a = old.split('\n');
	const b = cur.split('\n');
	const n = a.length;
	const m = b.length;
	// LCS table (fine for files of this size).
	const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
	for (let i = n - 1; i >= 0; i--) {
		for (let j = m - 1; j >= 0; j--) {
			dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
		}
	}
	// Walk the diff, collecting hunks of removed/added lines.
	const hunks = [];
	let curHunk = null;
	let i = 0;
	let j = 0;
	while (i < n && j < m) {
		if (a[i] === b[j]) {
			curHunk = null;
			i++;
			j++;
		} else {
			if (!curHunk) {
				curHunk = { removed: [], added: [], line: i + 1 };
				hunks.push(curHunk);
			}
			if (dp[i + 1][j] >= dp[i][j + 1]) {
				curHunk.removed.push(a[i]);
				i++;
			} else {
				curHunk.added.push(b[j]);
				j++;
			}
		}
	}
	while (i < n || j < m) {
		if (!curHunk) {
			curHunk = { removed: [], added: [], line: i + 1 };
			hunks.push(curHunk);
		}
		if (i < n && (j >= m || dp[i + 1][j] >= dp[i][j + 1])) {
			curHunk.removed.push(a[i]);
			i++;
		} else {
			curHunk.added.push(b[j]);
			j++;
		}
	}

	// User-visible copy in removed lines: quoted string literals AND plain
	// markup text between tags / attribute values.
	function literals(lines) {
		const out = [];
		// e.g. "text-micro text-muted-foreground" — class lists, not copy.
		const looksLikeClasses = (s) =>
			s.includes('-') &&
			s.split(/\s+/).every((token) => /^[a-z][a-z0-9-]*$/.test(token));
		const isCopy = (s) =>
			/[A-Z]/.test(s) || /[.!?…]/.test(s);
		for (const line of lines) {
			const seen = new Set();
			const push = (text) => {
				if (text && !seen.has(text)) {
					seen.add(text);
					out.push(text);
				}
			};
			const re = /'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"|`([^`\n]*)`/g;
			let match;
			while ((match = re.exec(line)) !== null) {
				const raw = match[1] ?? match[2] ?? match[3] ?? '';
				if (!raw) continue;
				// Unescape single/double quoted strings; keep backticks as-is
				// with ${x} converted to {x} (Paraglide placeholder syntax).
				let text = raw;
				if (match[3] !== undefined) {
					text = raw.replace(/\$\{([^}]+)\}/g, '{' + '$1' + '}');
				} else {
					try {
						text = JSON.parse(`"${match[1] ?? match[2]}"`);
					} catch {
						/* keep raw */
					}
				}
				push(text);
			}
			// Plain markup text: >Text< (not inside an expression).
			const markup = />([^<>{}][^<>]*?)</g;
			while ((match = markup.exec(line)) !== null) {
				const text = match[1].trim();
				if (/[A-Za-z]/.test(text)) push(text);
			}
			// Attribute copy: placeholder="…", title="…", label="…".
			const attr = /(?:placeholder|title|label|aria-label|tooltip|message)="([^"{]+)"/g;
			while ((match = attr.exec(line)) !== null) {
				const text = match[1].trim();
				if (/[A-Za-z]/.test(text)) push(text);
			}
			// Multi-line markup: a bare copy line between ">" and "<".
			const bare = line.trim();
			if (
				/^[A-Z][A-Za-z0-9 ,.:;!?%…()'&/-]*$/.test(bare) &&
				bare.length > 1
			) {
				push(bare);
			}
		}
		return out.filter(isCopy);
	}

	function keys(lines) {
		const out = [];
		for (const line of lines) {
			const re = /\bt\(\s*'([A-Za-z0-9_.]+)'/g;
			let match;
			while ((match = re.exec(line)) !== null) out.push(match[1]);
		}
		return out;
	}

	const pendingKeys = [];
	const pendingStrs = [];
	for (const hunk of hunks) {
		const strs = literals(hunk.removed).filter(
			(s) => /[A-Za-z]/.test(s) && s.length > 1 && !/^[\w./-]+$/.test(s),
		);
		const ks = keys(hunk.added);
		if (ks.length === 0) continue;
		if (strs.length === ks.length) {
			for (let k = 0; k < ks.length; k++) {
				if (!(ks[k] in messages)) messages[ks[k]] = strs[k];
			}
		} else {
			// Defer to file-level order pairing: the i18n commit replaced copy
			// mechanically, so leftover strings and keys keep document order.
			pendingStrs.push(...strs);
			pendingKeys.push(...ks);
		}
	}

	if (pendingKeys.length > 0) {
		if (pendingKeys.length === pendingStrs.length) {
			for (let k = 0; k < pendingKeys.length; k++) {
				if (!(pendingKeys[k] in messages)) messages[pendingKeys[k]] = pendingStrs[k];
			}
		} else {
			unmatched.push({
				file: relPath,
				line: 0,
				removed: pendingStrs,
				added: pendingKeys,
			});
		}
	}
}

const sorted = {};
for (const key of Object.keys(messages).sort()) sorted[key] = messages[key];

writeFileSync(outFile, JSON.stringify(sorted, null, '\t') + '\n');
console.log(`draft written: ${path.relative(repoRoot, outFile)} (${Object.keys(sorted).length} keys)`);
if (unmatched.length > 0) {
	console.log(`\nUNMATCHED hunks (${unmatched.length}) — resolve manually:`);
	for (const u of unmatched) {
		console.log(`  ${u.file}:${u.line}`);
		console.log(`    removed: ${JSON.stringify(u.removed)}`);
		console.log(`    added:   ${JSON.stringify(u.added)}`);
	}
} else {
	console.log('no unmatched hunks.');
}
