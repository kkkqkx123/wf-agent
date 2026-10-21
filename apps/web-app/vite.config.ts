import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { visualizer } from 'rollup-plugin-visualizer';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import adapter from '@sveltejs/adapter-auto';
import path from 'node:path';

export default defineConfig({
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
			alias: { '$lib': 'src/lib', '$paraglide': 'src/lib/paraglide' },
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
		alias: {
			$paraglide: path.resolve('./src/lib/paraglide'),
		},
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
});
