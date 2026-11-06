<script module lang="ts">
	// Single editor instance per component: switching files swaps the model
	// instead of rebuilding the whole editor, so cursor, scroll and workers
	// survive tab switches. Models live in the editor-models registry keyed
	// by file URI and are released when their tab closes.
	type Monaco = typeof import('monaco-editor');
	type MonacoEditorInstance = ReturnType<Monaco['editor']['create']>;
</script>

<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import {
		ensureIdeWorkers,
		getOrCreateFileModel,
		recallViewState,
		stashViewState,
	} from '#lib/ide/editor-models';
	import type { GutterMark } from '#lib/ide/git-diff-lines';
	import { t } from '#lib/i18n';

	interface Props {
		filePath: string;
		value: string;
		language?: string;
		readOnly?: boolean;
		contentReady?: boolean;
		revealLine?: number | null;
		revealCol?: number | null;
		gutterMarks?: GutterMark[];
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
		gutterMarks = [],
		onChange,
		onSave,
		onCursor,
	}: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let failed = $state(false);
	let loading = $state(true);
	let mountError = $state<string | null>(null);
	let retryNonce = $state(0);
	let editorReady = $state(false);
	let textareaValue = $state('');
	let editor: MonacoEditorInstance | null = null;
	let changeDisposable: { dispose(): void } | null = null;
	let cursorDisposable: { dispose(): void } | null = null;
	let decorationIds: string[] = [];
	let lastEmitted = $state('');
	let disposed = false;
	let lastRevealedKey = '';
	let attachedPath: string | null = null;

	let monacoNs: Monaco | null = null;

	async function ensureMonaco(): Promise<void> {
		try {
			await ensureIdeWorkers();
			const monaco = await import('monaco-editor');
			if (disposed) return;
			monacoNs = monaco;
			loading = false;
		} catch {
			if (!disposed) {
				failed = true;
				loading = false;
			}
		}
	}

	/** Create the single editor instance once the runtime and container exist. */
	function createEditor(): void {
		if (!monacoNs || !container) return;
		const instance = monacoNs.editor.create(container, {
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
		changeDisposable = instance.onDidChangeModelContent(() => {
			const next = instance.getValue();
			lastEmitted = next;
			onChange?.(next);
		});
		try {
			cursorDisposable = instance.onDidChangeCursorPosition((e) => {
				onCursor?.(e.position.lineNumber, e.position.column);
			});
		} catch {
			cursorDisposable = null;
		}
		try {
			instance.addCommand(monacoNs.KeyMod.CtrlCmd | monacoNs.KeyCode.KeyS, () => onSave?.());
		} catch {
			// Save shortcut inside the editor is best-effort.
		}
		editorReady = true;
	}

	/**
	 * Attach the file model to the persistent editor, preserving the
	 * previous file's view state and restoring this file's. Decorations
	 * belong to one model, so they reset on every switch and the gutter
	 * effect repopulates them.
	 */
	function attachModel(targetPath: string): void {
		if (!monacoNs || !editor) return;
		try {
			if (attachedPath && attachedPath !== targetPath) {
				try {
					stashViewState(attachedPath, editor.saveViewState());
				} catch {
					// View-state capture is best-effort.
				}
			}
			// Keystrokes must not re-attach: the value is read untracked so
			// this effect only depends on the file identity.
			const currentValue = untrack(() => value);
			const model = getOrCreateFileModel(monacoNs, targetPath, currentValue, language);
			editor.setModel(model);
			const recalled = recallViewState(targetPath);
			if (recalled) {
				try {
					editor.restoreViewState(recalled);
				} catch {
					// A stale view state never blocks the file from opening.
				}
			}
			decorationIds = [];
			attachedPath = targetPath;
			lastEmitted = model.getValue();
			mountError = null;
			const marks = untrack(() => gutterMarks);
			applyGutter(marks);
			if (typeof requestAnimationFrame !== 'undefined') {
				const settled = editor;
				requestAnimationFrame(() => {
					try {
						settled.layout();
					} catch {
						// Layout recovery is best-effort.
					}
				});
			}
		} catch (e) {
			mountError = e instanceof Error ? e.message : String(e);
		}
	}

	$effect(() => {
		const theme = resolvedTheme();
		if (editor && monacoNs) {
			monacoNs.editor.setTheme(theme === 'dark' ? 'vs-dark' : 'vs');
		}
	});

	$effect(() => {
		// Create once: the instance survives file switches, so this effect
		// only depends on runtime readiness, never on the file identity.
		// Reactive reads stay above the plain-variable guards: an effect
		// only subscribes to what it actually reads, so an early return on
		// `monacoNs`/`editor` alone would leave it deaf forever.
		const blocked = loading || failed;
		const host = container;
		void retryNonce;
		if (!monacoNs || blocked || !host || editor) return;
		try {
			createEditor();
			mountError = null;
		} catch (e) {
			mountError = e instanceof Error ? e.message : String(e);
		}
	});

	$effect(() => {
		// Attach only when switching files: keystrokes change `value` but
		// must never re-attach, which would reset the undo stack. Same
		// subscription rule as above: reactive reads stay above the guards.
		const currentPath = filePath;
		const blocked = loading || failed;
		void retryNonce;
		if (!monacoNs || blocked || !editor || !editorReady) return;
		attachModel(currentPath);
	});

	$effect(() => {
		const current = value;
		const blocked = failed || loading;
		if (!editor || blocked) return;
		if (current !== lastEmitted) {
			editor.setValue(current);
			lastEmitted = current;
		}
	});

	$effect(() => {
		const opts = {
			readOnly,
			fontSize: preferences.editorFontSize,
			wordWrap: (preferences.editorWordWrap ? 'on' : 'off') as 'on' | 'off',
		};
		if (!editor) return;
		editor.updateOptions(opts);
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

	$effect(() => {
		applyGutter(gutterMarks);
	});

	function applyGutter(marks: GutterMark[]): void {
		const instance = editor;
		if (!monacoNs || loading || failed || !instance) return;
		try {
			const RangeCtor = monacoNs.Range;
			const lane = monacoNs.editor.OverviewRulerLane.Right;
			const decs = marks.map((mark: GutterMark) => ({
				range: new RangeCtor(mark.line, 1, mark.line, 1),
				options: {
					linesDecorationsClassName:
						mark.kind === 'add'
							? 'ide-gutter-add'
							: mark.kind === 'change'
								? 'ide-gutter-change'
								: 'ide-gutter-delete',
					hoverMessage: mark.hover
						? { value: ['```diff', mark.hover, '```'].join('\n') }
						: undefined,
					overviewRuler: {
						color: mark.kind === 'add' ? '#22c55e' : mark.kind === 'change' ? '#f59e0b' : '#ef4444',
						position: lane,
					},
				},
			}));
			decorationIds = instance.deltaDecorations(decorationIds, decs);
		} catch {
			// Gutter decorations are advisory; the file stays editable.
		}
	}

	function revealAt(line: number, col?: number | null): void {
		try {
			if (!editor) return;
			const model = editor.getModel();
			const lineCount = model?.getLineCount() ?? null;
			const safeLine =
				typeof lineCount === 'number'
					? Math.min(Math.max(1, Math.floor(line)), Math.max(1, lineCount))
					: Math.max(1, Math.floor(line));
			let safeCol = Math.max(1, Math.floor(col ?? 1));
			try {
				if (model) {
					const maxCol = model.getLineMaxColumn(safeLine);
					if (Number.isFinite(maxCol)) safeCol = Math.min(safeCol, Math.max(1, maxCol));
				}
			} catch {
				// Column clamping is best-effort.
			}
			editor?.revealLineInCenter(safeLine);
			editor?.setPosition({ lineNumber: safeLine, column: safeCol });
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
		// Models belong to their tabs and are released when the tab closes;
		// only the transient view state is stashed here.
		if (editor && attachedPath) {
			try {
				stashViewState(attachedPath, editor.saveViewState());
			} catch {
				// View-state capture is best-effort.
			}
		}
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
	<div class="flex h-full min-h-0 flex-col">
		{#if mountError}
			<div
				class="flex items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 py-1 text-micro text-destructive"
			>
				<span class="min-w-0 flex-1">{mountError}</span>
				<button
					type="button"
					class="cursor-pointer underline hover:no-underline"
					onclick={() => {
						mountError = null;
						retryNonce += 1;
					}}
				>
					{t('common.retry')}
				</button>
			</div>
		{/if}
		<div bind:this={container} class="h-full min-h-64 w-full flex-1 overflow-hidden"></div>
	</div>
{/if}
