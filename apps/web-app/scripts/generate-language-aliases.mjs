/**
 * Regenerates src/lib/utils/language-aliases.generated.ts from the shared
 * alias table in crates/infra/wf-syntax/src/language-aliases.tsv. The TSV
 * file is the single source of truth; run this script after editing it and
 * commit both files. The vitest suite keeps the generated module fresh.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const tsvPath = join(here, '../../../crates/infra/wf-syntax/src/language-aliases.tsv');
const outPath = join(here, '../src/lib/utils/language-aliases.generated.ts');

const rows = readFileSync(tsvPath, 'utf8')
	.split('\n')
	.map((line) => line.trim())
	.filter((line) => line !== '' && !line.startsWith('#'))
	.map((line) => {
		const tab = line.indexOf('\t');
		if (tab < 0) throw new Error(`alias line missing tab separator: ${line}`);
		const alias = line.slice(0, tab).trim();
		const canonical = line.slice(tab + 1).trim();
		if (!alias || !canonical) throw new Error(`blank field in: ${line}`);
		return { alias, canonical };
	});

const entries = rows.map(({ alias, canonical }) => `\t${JSON.stringify(alias)}: ${JSON.stringify(canonical)},`);

const banner = `/**
 * GENERATED FILE - do not edit by hand.
 * Source of truth: crates/infra/wf-syntax/src/language-aliases.tsv
 * Regenerate with: npm run gen:aliases
 */

`;

writeFileSync(
	outPath,
	`${banner}export const LANGUAGE_ALIASES: Record<string, string> = {
${entries.join('\n')}
};
`,
);

console.log(`Wrote ${rows.length} aliases to ${outPath}`);
