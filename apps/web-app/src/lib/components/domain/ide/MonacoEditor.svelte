<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { resolvedTheme } from '#lib/stores/theme.svelte';

	interface Props {
		filePath: string;
		value: string;
		language?: string;
		readOnly?: boolean;
		onChange?: (value: string) => void;
	}

	let { filePath, value, language = 'plaintext', readOnly = false, onChange }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let failed = $state(false);
	let loading = $state(true);
	let textareaValue = $state('');
	let editor: { dispose(): void } | null = null;
	let changeDisposable: { dispose(): void } | null = null;
	let lastEmitted = $state('');
	let disposed = false;

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	type AnyMonaco = any;

	let monacoNs: AnyMonaco | null = null;
	const models = new SvelteMap<string, AnyMonaco>();

	async function ensureMonaco(): Promise<void> {
		try {
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
		const key = `ide:${targetPath}`;
		let model = models.get(key);
		if (!model) {
			model = monacoNs.editor.createModel(value, language);
			models.set(key, model);
		} else {
			monacoNs.editor.setModelLanguage(model, language);
			if (model.getValue() !== value) model.setValue(value);
		}
		const instance = monacoNs.editor.create(container, {
			model,
			readOnly,
			automaticLayout: true,
			minimap: { enabled: false },
			fontSize: 13,
			fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace",
			scrollBeyondLastLine: false,
			theme: resolvedTheme() === 'dark' ? 'vs-dark' : 'vs',
		});
		editor = instance;
		lastEmitted = value;
		changeDisposable = instance.onDidChangeModelContent(() => {
			const next = instance.getValue() as string;
			lastEmitted = next;
			textareaValue = next;
			onChange?.(next);
		});
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
			textareaValue = value;
		}
	});

	$effect(() => {
		if (!editor) return;
		(editor as unknown as { updateOptions(o: unknown): void }).updateOptions({ readOnly });
	});

	$effect(() => {
		// Remount only when switching files so each file keeps its own model.
		const currentPath = filePath;
		if (!monacoNs || loading || failed) return;
		untrack(() => mountFor(currentPath));
	});

	onMount(() => {
		textareaValue = value;
		void ensureMonaco();
	});

	onDestroy(() => {
		disposed = true;
		changeDisposable?.dispose();
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
