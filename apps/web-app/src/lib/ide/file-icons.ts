import type { IconName } from '#lib/ui/icons/paths';

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif']);

const TERMINAL_EXTS = new Set(['sh', 'bash', 'zsh', 'fish', 'ps1', 'bat', 'cmd']);

const DATABASE_EXTS = new Set(['sql', 'db', 'sqlite']);

const CODE_EXTS = new Set([
	'ts',
	'mts',
	'cts',
	'tsx',
	'js',
	'mjs',
	'cjs',
	'jsx',
	'svelte',
	'vue',
	'astro',
	'rs',
	'py',
	'go',
	'java',
	'c',
	'h',
	'cpp',
	'hpp',
	'cs',
	'php',
	'rb',
	'swift',
	'kotlin',
	'css',
	'scss',
	'less',
	'html',
	'htm',
]);

function extensionOf(path: string): string {
	const base = path.split('/').pop() ?? path;
	const dot = base.lastIndexOf('.');
	if (dot <= 0 || dot === base.length - 1) return '';
	return base.slice(dot + 1).toLowerCase();
}

/** Shape-only icon for a file path. Unknown types fall back to a plain file. */
export function iconForFile(path: string): IconName {
	const ext = extensionOf(path);
	if (IMAGE_EXTS.has(ext)) return 'image';
	if (TERMINAL_EXTS.has(ext)) return 'terminal';
	if (DATABASE_EXTS.has(ext)) return 'database';
	if (CODE_EXTS.has(ext)) return 'code';
	return 'file';
}

/** Directory entries always use the folder shape. */
export function iconForEntry(path: string, isDir: boolean): IconName {
	if (isDir) return 'folder';
	return iconForFile(path);
}
