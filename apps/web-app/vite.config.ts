import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { visualizer } from 'rollup-plugin-visualizer';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import adapter from '@sveltejs/adapter-static';

const rootDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(({ mode }) => {
	const isPreview = mode === 'preview';
	const analyze = process.env.ANALYZE === '1';
	const alias: Record<string, string> = isPreview
		? {
				'#lib/api/client': path.resolve(rootDir, 'src/lib/api/client.preview.ts'),
			}
		: {};
	return {
		plugins: [
			tailwindcss(),
			paraglideVitePlugin({
				project: './project.inlang',
				outdir: './src/lib/paraglide',
				strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
			}),
			sveltekit({
				preprocess: vitePreprocess(),
				// Pure client-side SPA (ssr = false in +layout.ts): emit a
				// static bundle with an index.html fallback for client routing.
				adapter: adapter({ fallback: 'index.html' }),
			}),
			...(analyze
				? [
						visualizer({
							filename: 'stats.md',
							template: 'markdown',
							emitFile: true,
							gzipSize: true,
							brotliSize: true,
						}),
					]
				: []),
		],
		resolve: {
			alias,
		},
		build: {
			// Chunks here are on-demand lazy imports (Monaco, cytoscape,
			// Shiki core and individual language grammars), fetched only when
			// their feature is used, so the default 500 kB threshold is noise.
			// 2500 kB sits just above the largest legitimate chunk (the Monaco
			// editor core) and still flags anything genuinely oversized.
			chunkSizeWarningLimit: 2500,
		},
		server: {
			host: '0.0.0.0',
			port: 3017,
			proxy: {
				'/api': {
					target: process.env.VITE_API_TARGET ?? 'http://localhost:3000',
					changeOrigin: true,
				},
			},
		},
		test: {
			// Unit tests co-locate with sources; integration tests live in
			// tests/. Explicit scope so a package-level runner never sweeps
			// unrelated directories (build output, scripts, message fragments).
			include: ['src/**/*.test.ts', 'tests/**/*.test.ts'],
			environment: 'node',
			fsModuleCache: true,
		},
	};
});
