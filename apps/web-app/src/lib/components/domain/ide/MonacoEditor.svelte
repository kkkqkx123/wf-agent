<script module lang="ts">
	// Module-level model cache so switching tabs reuses models instead of
	// re-parsing files. Bounded to keep memory stable when many files open.
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	type AnyMonaco = any;

	import { SvelteMap } from 'svelte/reactivity';

	const ideModels = new SvelteMap<string, AnyMonaco>();
	const ideModelOrder: string[] = [];
	const MAX_IDE_MODELS = 20;

	function touchIdeModel(key: string): void {
		const index = ideModelOrder.indexOf(key);
		if (index >= 0) ideModelOrder.splice(index, 1);
		ideModelOrder.push(key);
	}

	function rememberIdeModel(key: string, model: AnyMonaco): void {
		ideModels.set(key, model);
		touchIdeModel(key);
		while (ideModelOrder.length > MAX_IDE_MODELS) {
			const oldest = ideModelOrder.shift();
			if (oldest === undefined || oldest === key) continue;
			const evicted = ideModels.get(oldest);
			ideModels.delete(oldest);
			try {
				evicted?.dispose();
			} catch {
				// Disposal is best-effort; the cache entry is already dropped.
			}
		}
	}
</script>

<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';

	interface Props {
		filePath: string;
		value: string;
		language?: string;
		readOnly?: boolean;
		contentReady?: boolean;
		revealLine?: number | null;
		revealCol?: number | null;
		onChange?: (value: string) => void;
		onSave?: () => void;
		onCursor?: (line: number, col: number) => void;
	}

	let {
		filePath,
		value,
		language = 'plaintext',
		readOnly = false,
		contentReady = true,
		revealLine = null,
		revealCol = null,
		onChange,
		onSave,
		onCursor,
	}: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let failed = $state(false);
	let loading = $state(true);
	let textareaValue = $state('');
	let editor: { dispose(): void } | null = null;
	let changeDisposable: { dispose(): void } | null = null;
	let cursorDisposable: { dispose(): void } | null = null;
	let lastEmitted = $state('');
	let disposed = false;
	let lastRevealedKey = '';

	let monacoNs: AnyMonaco | null = null;

	let monacoWorkersReady = false;

	/** Register Monaco web workers before the editor loads. Best-effort:
	 * worker failure only costs local syntax and suggestion features. */
	async function ensureMonacoWorkers(): Promise<void> {
		if (monacoWorkersReady) return;
		if (import.meta.env['VITE_MONACO_NO_WORKERS'] === '1') return;
		try {
			const [
				{ default: EditorWorker },
				{ default: TsWorker },
				{ default: CssWorker },
				{ default: HtmlWorker },
				{ default: JsonWorker },
			] = await Promise.all([
				import('monaco-editor/esm/vs/editor/editor.worker?worker'),
				import('monaco-editor/esm/vs/language/typescript/ts.worker?worker'),
				import('monaco-editor/esm/vs/language/css/css.worker?worker'),
				import('monaco-editor/esm/vs/language/html/html.worker?worker'),
				import('monaco-editor/esm/vs/language/json/json.worker?worker'),
			]);
			(
				globalThis as unknown as {
					MonacoEnvironment?: { getWorker(workerId: unknown, label: string): Worker };
				}
			).MonacoEnvironment = {
				getWorker(_workerId: unknown, label: string): Worker {
					if (label === 'json') return new JsonWorker();
					if (label === 'css' || label === 'scss' || label === 'less')
						return new CssWorker();
					if (
						label === 'html' ||
						label === 'handlebars' ||
						label === 'razor'
					)
						return new HtmlWorker();
					if (label === 'typescript' || label === 'javascript') return new TsWorker();
					return new EditorWorker();
				},
			};
			monacoWorkersReady = true;
		} catch {
			// Keep the no-worker path; the editor still opens with degraded
			// language features.
		}
	}

	async function ensureMonaco(): Promise<void> {
		try {
			await ensureMonacoWorkers();
			const monaco = await import('monaco-editor');
			if (disposed) return;
			monacoNs = monaco;
			loading = false;
			mount();
		} catch {
			if (!disposed) {
				failed = true;
				loading = false;
			}
		}
	}

	function mount(): void {
		mountFor(filePath);
	}

	function mountFor(targetPath: string): void {
		if (!monacoNs || !container) return;
		editor?.dispose();
		changeDisposable?.dispose();
		cursorDisposable?.dispose();
		const key = `ide:${targetPath}`;
		let model = ideModels.get(key);
		if (!model) {
			model = monacoNs.editor.createModel(value, language);
			rememberIdeModel(key, model);
		} else {
			touchIdeModel(key);
			monacoNs.editor.setModelLanguage(model, language);
			if (model.getValue() !== value) model.setValue(value);
		}
		const instance = monacoNs.editor.create(container, {
			model,
			readOnly,
			automaticLayout: true,
			minimap: { enabled: false },
			fontSize: preferences.editorFontSize,
			wordWrap: preferences.editorWordWrap ? 'on' : 'off',
			fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace",
			scrollBeyondLastLine: false,
			theme: resolvedTheme() === 'dark' ? 'vs-dark' : 'vs',
		});
		editor = instance;
		lastEmitted = value;
		changeDisposable = instance.onDidChangeModelContent(() => {
			const next = instance.getValue() as string;
			lastEmitted = next;
			if (failed) textareaValue = next;
			onChange?.(next);
		});
		try {
			cursorDisposable = instance.onDidChangeCursorPosition?.((e: {
				position: { lineNumber: number; column: number };
			}) => {
				onCursor?.(e.position.lineNumber, e.position.column);
			});
		} catch {
			cursorDisposable = null;
		}
		try {
			instance.addCommand?.(
				(monacoNs.KeyMod.CtrlCmd | monacoNs.KeyCode.KeyS) as number,
				() => onSave?.(),
			);
		} catch {
			// Save shortcut inside the editor is best-effort.
		}
	}

	$effect(() => {
		const theme = resolvedTheme();
		if (editor && monacoNs) {
			monacoNs.editor.setTheme(theme === 'dark' ? 'vs-dark' : 'vs');
		}
	});

	$effect(() => {
		if (!editor || failed || loading) return;
		if (value !== lastEmitted) {
			(editor as unknown as { setValue(v: string): void }).setValue(value);
			lastEmitted = value;
		}
	});

	$effect(() => {
		if (!editor) return;
		(editor as unknown as { updateOptions(o: unknown): void }).updateOptions({
			readOnly,
			fontSize: preferences.editorFontSize,
			wordWrap: preferences.editorWordWrap ? 'on' : 'off',
		});
	});

	$effect(() => {
		// Remount only when switching files so each file keeps its own model.
		const currentPath = filePath;
		if (!monacoNs || loading || failed) return;
		untrack(() => mountFor(currentPath));
	});

	$effect(() => {
		// Reveal after the file finished loading so the position is not lost.
		// Keystrokes must not re-trigger this: the effect only depends on the
		// load-ready flag, never on the edited value.
		const line = revealLine;
		const col = revealCol;
		const path = filePath;
		const ready = contentReady;
		if (!monacoNs || loading || failed || !editor || !ready || line === null || line === undefined)
			return;
		const key = `${path}:${String(line)}:${String(col ?? '')}`;
		if (key === lastRevealedKey) return;
		lastRevealedKey = key;
		untrack(() => revealAt(line, col));
	});

	function revealAt(line: number, col?: number | null): void {
		try {
			const instance = editor as unknown as {
				revealLineInCenter(line: number): void;
				setPosition(pos: { lineNumber: number; column: number }): void;
			} | null;
			if (!instance) return;
			const model = monacoNs?.editor?.getModels?.()?.find?.(
				// eslint-disable-next-line @typescript-eslint/no-explicit-any
				(m: any) => m?.uri?.path?.endsWith?.(filePath),
			);
			const lineCount =
				typeof model?.getLineCount === 'function' ? (model.getLineCount() as number) : null;
			const safeLine =
				typeof lineCount === 'number'
					? Math.min(Math.max(1, Math.floor(line)), Math.max(1, lineCount))
					: Math.max(1, Math.floor(line));
			let safeCol = Math.max(1, Math.floor(col ?? 1));
			try {
				if (typeof model?.getLineMaxColumn === 'function') {
					const maxCol = model.getLineMaxColumn(safeLine) as number;
					if (Number.isFinite(maxCol)) safeCol = Math.min(safeCol, Math.max(1, maxCol));
				}
			} catch {
				// Column clamping is best-effort.
			}
			instance.revealLineInCenter(safeLine);
			instance.setPosition({ lineNumber: safeLine, column: safeCol });
		} catch {
			// Reveal is best-effort; the file stays open without positioning.
		}
	}

	onMount(() => {
		textareaValue = value;
		void ensureMonaco();
	});

	onDestroy(() => {
		disposed = true;
		changeDisposable?.dispose();
		cursorDisposable?.dispose();
		editor?.dispose();
		editor = null;
	});
</script>

{#if loading}
	<div class="flex h-full items-center justify-center text-caption text-muted-foreground">
		Loading editor…
	</div>
{:else if failed}
	<textarea
		class="h-full min-h-64 w-full resize-none rounded-md border border-input bg-card p-3 font-mono text-body"
		bind:value={textareaValue}
		readonly={readOnly}
		spellcheck={false}
		oninput={(e) => onChange?.((e.target as HTMLTextAreaElement).value)}></textarea>
{:else}
	<div bind:this={container} class="h-full min-h-64 w-full overflow-hidden"></div>
{/if}
