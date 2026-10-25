import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { visualizer } from 'rollup-plugin-visualizer';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import adapter from '@sveltejs/adapter-auto';

const rootDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(({ mode }) => {
	const isPreview = mode === 'preview';
	const alias: Record<string, string> = isPreview
		? {
				'#lib/api/client': path.resolve(
					rootDir,
					'src/lib/api/client.preview.ts',
				),
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
				adapter: adapter(),
			}),
			visualizer({
				filename: 'stats.md',
				template: 'markdown',
				emitFile: true,
				gzipSize: true,
				brotliSize: true,
			}),
		],
	resolve: {
			alias,
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
	};
});
