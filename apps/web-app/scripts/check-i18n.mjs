// Compares message keys across locales so en/zh never drift apart.
// Usage: node scripts/check-i18n.mjs
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

const messagesDir = path.resolve(process.cwd(), 'messages');

const files = readdirSync(messagesDir).filter((f) => f.endsWith('.json'));
if (files.length === 0) {
	console.error('No message files found in', messagesDir);
	process.exit(1);
}

const keySets = new Map();
for (const file of files) {
	const parsed = JSON.parse(readFileSync(path.join(messagesDir, file), 'utf8'));
	keySets.set(file, new Set(Object.keys(parsed)));
}

const [baseFile, ...rest] = [...keySets.keys()].sort();
const base = keySets.get(baseFile);
let failed = false;

for (const file of rest) {
	const keys = keySets.get(file);
	for (const key of base) {
		if (!keys.has(key)) {
			console.error(`${file}: missing key "${key}"`);
			failed = true;
		}
	}
	for (const key of keys) {
		if (!base.has(key)) {
			console.error(`${file}: unknown key "${key}" (not in ${baseFile})`);
			failed = true;
		}
	}
}

if (failed) {
	process.exit(1);
}
console.log(`i18n check OK: ${files.length} locales, ${base.size} keys each.`);
