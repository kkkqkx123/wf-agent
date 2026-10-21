// Merges messages/fragments/*.{locale}.json into messages/{locale}.json.
// Fragment files are the source of truth; the top-level locale files are
// generated artifacts that Paraglide compiles from. Idempotent: running it
// twice produces byte-identical output.
//
// Usage: node scripts/merge-fragments.mjs
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

const messagesDir = path.resolve(process.cwd(), 'messages');
const fragmentsDir = path.join(messagesDir, 'fragments');

const LOCALES = ['en', 'zh'];

function readJson(file) {
	return JSON.parse(readFileSync(file, 'utf8'));
}

// 2-space indent, trailing newline, keys sorted — matches the existing files.
function render(obj) {
	const sorted = {};
	for (const key of Object.keys(obj).sort()) {
		sorted[key] = obj[key];
	}
	return JSON.stringify(sorted, null, '\t') + '\n';
}

const fragmentFiles = readdirSync(fragmentsDir).filter((f) =>
	f.endsWith('.json'),
);

// Group fragment files by locale and check en/zh key parity per fragment.
const byLocale = Object.fromEntries(LOCALES.map((l) => [l, []]));
for (const file of fragmentFiles) {
	const match = file.match(/^(.*)\.(en|zh)\.json$/);
	if (!match) {
		console.error(`fragments/${file}: filename does not match <name>.<locale>.json`);
		process.exit(1);
	}
	byLocale[match[2]].push(match);
}

const names = new Set(byLocale.en.map(([, name]) => name));
for (const locale of LOCALES) {
	for (const [, name] of byLocale[locale]) {
		if (!names.has(name)) {
			console.error(`fragments/${name}.${locale}.json: missing counterpart locale file`);
			process.exit(1);
		}
	}
}

for (const name of [...names].sort()) {
	const keySets = LOCALES.map((locale) =>
		new Set(Object.keys(readJson(path.join(fragmentsDir, `${name}.${locale}.json`)))),
	);
	const [enKeys, zhKeys] = keySets;
	let failed = false;
	for (const key of enKeys) {
		if (!zhKeys.has(key)) {
			console.error(`fragments/${name}.zh.json: missing key "${key}"`);
			failed = true;
		}
	}
	for (const key of zhKeys) {
		if (!enKeys.has(key)) {
			console.error(`fragments/${name}.en.json: unknown key "${key}"`);
			failed = true;
		}
	}
	if (failed) process.exit(1);
}

// Merge: base top-level file, then every fragment on top. Fragments win on
// conflict so they remain the source of truth.
for (const locale of LOCALES) {
	const target = path.join(messagesDir, `${locale}.json`);
	const merged = readJson(target);
	for (const [, name] of byLocale[locale]) {
		Object.assign(merged, readJson(path.join(fragmentsDir, `${name}.${locale}.json`)));
	}
	writeFileSync(target, render(merged));
	console.log(`merged ${names.size} fragments into messages/${locale}.json`);
}
console.log(`i18n fragment merge OK: ${names.size} fragments, ${LOCALES.length} locales.`);
