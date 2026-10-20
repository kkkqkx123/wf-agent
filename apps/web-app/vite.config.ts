import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit(),
		visualizer({
			filename: 'stats.md',
			template: 'markdown',
			emitFile: true,
			gzipSize: true,
			brotliSize: true,
		}),
	],
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
