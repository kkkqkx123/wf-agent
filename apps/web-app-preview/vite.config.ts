import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
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
