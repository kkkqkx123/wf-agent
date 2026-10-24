import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default defineConfig(
	js.configs.recommended,
	ts.configs.recommended,
	svelte.configs.recommended,
	{
		languageOptions: {
			globals: {
				...globals.browser,
				...globals.node,
			},
		},
	},
	{
		files: ['**/*.svelte', '**/*.svelte.js', '**/*.svelte.ts'],
		languageOptions: {
			parserOptions: {
				parser: ts.parser,
				extraFileExtensions: ['.svelte'],
			},
		},
		rules: {
			// These files are parsed through TypeScript, which resolves names
			// itself; the base no-undef rule cannot see type-only globals such
			// as DOM lib types and reports them as undefined.
			'no-undef': 'off',
		},
	},
	{
		ignores: ['.svelte-kit/**', 'build/**', 'src/lib/paraglide/**'],
	},
);
