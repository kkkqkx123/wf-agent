<script module lang="ts">
	type Monaco = typeof import('monaco-editor');
	type MonacoDiffEditorInstance = ReturnType<Monaco['editor']['createDiffEditor']>;
	type MonacoDiffModel = ReturnType<Monaco['editor']['createModel']>;
</script>

<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import {
		ensureIdeWorkers,
		getOrCreateDiffOriginalModel,
		getOrCreateFileModel,
		recallDiffViewState,
		releaseDiffOriginalModel,
		stashDiffViewState,
	} from '#lib/ide/editor-models';
	import { t } from '#lib/i18n';

	interface Props {
		diffKey: string;
		/** Null for transient read-only modified sides (staged/commit). */
		filePath: string | null;
		originalText: string;
		modifiedText: string;
		language?: string;
		editable?: boolean;
		onChange?: (value: string) => void;
		onSave?: () => void;
	}

	let {
		diffKey,
		filePath,
		originalText,
		modifiedText,
		language = 'plaintext',
		editable = false,
		onChange,
		onSave,
	}: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let failed = $state(false);
	let loading = $state(true);
	let mountError = $state<string | null>(null);
	let editor: MonacoDiffEditorInstance | null = null;
	let changeDisposable: { dispose(): void } | null = null;
	let transientModified: MonacoDiffModel | null = null;
	let lastEmitted = $state('');
	let disposed = false;
	let monacoNs: Monaco | null = null;

	function readOnly(): boolean {
		return !editable;
	}

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

	function createEditor(): void {
		if (!monacoNs || !container) return;
		const instance = monacoNs.editor.createDiffEditor(container, {
			readOnly: readOnly(),
			originalEditable: false,
			renderSideBySide: true,
			automaticLayout: true,
			minimap: { enabled: false },
			fontSize: preferences.editorFontSize,
			wordWrap: preferences.editorWordWrap ? 'on' : 'off',
			fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace",
			scrollBeyondLastLine: false,
			theme: resolvedTheme() === 'dark' ? 'vs-dark' : 'vs',
		});
		editor = instance;
		attachModels();
		try {
			const modified = instance.getModifiedEditor();
			changeDisposable = modified.onDidChangeModelContent(() => {
				if (!editable) return;
				const next = modified.getValue();
				lastEmitted = next;
				onChange?.(next);
			});
			modified.addCommand(monacoNs.KeyMod.CtrlCmd | monacoNs.KeyCode.KeyS, () => onSave?.());
		} catch {
			changeDisposable = null;
		}
	}

	/** Original side is always tab-local; the modified side reuses the
	 * shared file model only when the diff is editable. */
	function attachModels(): void {
		if (!monacoNs || !editor) return;
		try {
			const currentOriginal = untrack(() => originalText);
			const currentModified = untrack(() => modifiedText);
			const currentLanguage = untrack(() => language);
			const original = getOrCreateDiffOriginalModel(
				monacoNs,
				diffKey,
				currentOriginal,
				currentLanguage,
			);
			let modified;
			if (filePath) {
				modified = getOrCreateFileModel(monacoNs, filePath, currentModified, currentLanguage);
			} else {
				transientModified?.dispose();
				transientModified = monacoNs.editor.createModel(currentModified, currentLanguage);
				modified = transientModified;
			}
			editor.setModel({ original, modified });
			lastEmitted = modified.getValue();
			const recalled = recallDiffViewState(diffKey);
			if (recalled) {
				try {
					editor.restoreViewState(recalled);
				} catch {
					// A stale view state never blocks the diff from opening.
				}
			}
			mountError = null;
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
		// Create once: the instance survives option updates, so this effect
		// only depends on runtime readiness, never on the compared texts.
		const blocked = loading || failed;
		const host = container;
		if (!monacoNs || blocked || !host || editor) return;
		try {
			createEditor();
			mountError = null;
		} catch (e) {
			mountError = e instanceof Error ? e.message : String(e);
		}
	});

	$effect(() => {
		const theme = resolvedTheme();
		if (editor && monacoNs) {
			monacoNs.editor.setTheme(theme === 'dark' ? 'vs-dark' : 'vs');
		}
	});

	$effect(() => {
		const opts = {
			readOnly: readOnly(),
			fontSize: preferences.editorFontSize,
			wordWrap: (preferences.editorWordWrap ? 'on' : 'off') as 'on' | 'off',
		};
		if (!editor) return;
		editor.updateOptions(opts);
	});

	$effect(() => {
		// External edits (save in the file tab, reload) sync the modified
		// side; keystrokes update the store first so this stays a no-op.
		const current = modifiedText;
		const blocked = failed || loading;
		if (!editor || blocked) return;
		if (current !== lastEmitted) {
			editor.getModifiedEditor().setValue(current);
			lastEmitted = current;
		}
	});

	$effect(() => {
		// A refreshed base swaps the original side without touching edits.
		const current = originalText;
		const blocked = failed || loading;
		if (!editor || blocked || !monacoNs) return;
		const model = editor.getOriginalEditor().getModel();
		if (model && model.getValue() !== current) model.setValue(current);
	});

	onMount(() => {
		void ensureMonaco();
	});

	onDestroy(() => {
		disposed = true;
		if (editor) {
			try {
				stashDiffViewState(diffKey, editor.saveViewState());
			} catch {
				// View-state capture is best-effort.
			}
		}
		changeDisposable?.dispose();
		editor?.dispose();
		editor = null;
		transientModified?.dispose();
		transientModified = null;
		releaseDiffOriginalModel(diffKey);
	});
</script>

{#if loading}
	<div class="flex h-full items-center justify-center text-caption text-muted-foreground">
		Loading diff…
	</div>
{:else if failed}
	<p class="p-4 text-caption text-muted-foreground">{t('page.ide.diffEditorUnavailable')}</p>
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
						attachModels();
					}}
				>
					{t('common.retry')}
				</button>
			</div>
		{/if}
		<div bind:this={container} class="h-full min-h-64 w-full flex-1 overflow-hidden"></div>
	</div>
{/if}
