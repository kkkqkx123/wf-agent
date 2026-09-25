import { loadEnv } from 'vite';
import { defineConfig } from 'vitest/config';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, process.cwd(), '');
	const backendTarget = env.VITE_BACKEND_TARGET ?? 'http://127.0.0.1:8080';

	return {
		plugins: [tailwindcss(), sveltekit()],
		server: {
			proxy: {
				'/api': { target: backendTarget, changeOrigin: true },
				'/health': { target: backendTarget, changeOrigin: true },
				'/info': { target: backendTarget, changeOrigin: true },
				'/api-docs': { target: backendTarget, changeOrigin: true },
				'/ws': { target: backendTarget, ws: true, changeOrigin: true },
			},
		},
		test: {
			include: ['{src,tests}/**/*.{test,spec}.{js,ts}'],
		},
	};
});
