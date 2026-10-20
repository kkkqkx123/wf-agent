import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { visualizer } from 'rollup-plugin-visualizer';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import path from 'node:path';

export default defineConfig({
	plugins: [
		tailwindcss(),
		paraglideVitePlugin({
			project: './project.inlang',
			outdir: './src/lib/paraglide',
			// Client-rendered app: no server-visible strategy is needed.
			strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
		}),
		sveltekit(),
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
