<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import DiffView from '#lib/ui/components/DiffView.svelte';
	import MonacoDiffEditor from './MonacoDiffEditor.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import { getDiff, getDiffRange, getGitFile, getStagedDiff, runAction } from '#lib/services/git';
	import { readFsFile, languageForPath } from '#lib/services/fs';
	import type { DiffBase } from '#lib/ide/diff-tabs';
	import { diffEditable } from '#lib/ide/diff-tabs';
	import type { DiffFile } from '#lib/types/diff';
	import { copyPlainText } from '#lib/utils/clipboard';
	import { diffFilesToText } from '#lib/types/diff';
	import { t } from '#lib/i18n';

	interface Props {
		diffKey: string;
		path: string;
		base: DiffBase;
		onOpenInGit?: () => void;
	}

	let { diffKey, path, base, onOpenInGit }: Props = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let hunks = $state<DiffFile[]>([]);
	let originalText = $state('');
	let originalReady = $state(false);
	let originalMissing = $state(false);
	let modifiedText = $state('');
	let modifiedReady = $state(false);
	let binary = $state(false);
	let truncated = $state(false);
	let staging = $state(false);

	const editable = $derived(diffEditable(base));
	const language = $derived(languageForPath(path));

	/** Full-text view needs both sides present, decodable and uncut. */
	const fullText = $derived(
		originalReady && modifiedReady && !originalMissing && !binary && !truncated,
	);

	async function load(): Promise<void> {
		loading = true;
		error = null;
		hunks = [];
		originalReady = false;
		modifiedReady = false;
		originalMissing = false;
		binary = false;
		truncated = false;
		try {
			if (base.kind === 'unstaged') {
				await ide.ensureFileTab(path);
				const tab = ide.tabs.find((entry) => entry.path === path);
				if (tab?.isBinary) {
					binary = true;
					hunks = await getDiff(path);
				} else {
					const [original, rows] = await Promise.all([getGitFile(path), getDiff(path)]);
					originalMissing = !original.exists;
					originalText = original.content;
					originalReady = true;
					binary = original.is_binary;
					truncated = original.truncated;
					modifiedText =
						tab && !tab.loading ? tab.content : ((await readFsFile(path)).content ?? '');
					modifiedReady = true;
					hunks = rows.filter((row) => row.path === path);
				}
			} else if (base.kind === 'staged') {
				const [original, modified, rows] = await Promise.all([
					getGitFile(path),
					getGitFile(path, ':staged'),
					getStagedDiff(path),
				]);
				originalMissing = !original.exists && !modified.exists;
				originalText = original.exists ? original.content : '';
				originalReady = true;
				modifiedText = modified.exists ? modified.content : '';
				modifiedReady = true;
				binary = original.is_binary || modified.is_binary;
				truncated = original.truncated || modified.truncated;
				hunks = rows.filter((row) => row.path === path);
			} else {
				const [original, modified, rows] = await Promise.all([
					getGitFile(path, base.parent),
					getGitFile(path, base.commit),
					getDiffRange(base.parent, base.commit, path),
				]);
				originalMissing = !original.exists && !modified.exists;
				originalText = original.exists ? original.content : '';
				originalReady = true;
				modifiedText = modified.exists ? modified.content : '';
				modifiedReady = true;
				binary = original.is_binary || modified.is_binary;
				truncated = original.truncated || modified.truncated;
				hunks = rows.filter((row) => row.path === path);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void diffKey;
		void path;
		void ide.gitRefreshSeq;
		void load();
	});

	function stageLabel(): string {
		return base.kind === 'staged' ? t('page.ide.unstage') : t('page.ide.stage');
	}

	async function toggleStage(): Promise<void> {
		if (staging || (base.kind !== 'unstaged' && base.kind !== 'staged')) return;
		staging = true;
		try {
			await runAction('stage.toggle', { path });
			await ide.refreshGitStatus();
			ide.bumpGitRefresh();
		} catch (e) {
			toasts.error(stageLabel(), e instanceof Error ? e.message : undefined);
		} finally {
			staging = false;
		}
	}

	async function copyDiffText(): Promise<void> {
		const text = diffFilesToText(hunks.map((file) => ({ title: file.path, file })));
		if (!text) return;
		if (await copyPlainText(text)) {
			toasts.success(t('common.copied'));
		} else {
			toasts.error(t('common.copyFailed'), t('common.copyFailedDetail'));
		}
	}

	async function saveFromDiff(): Promise<void> {
		const ok = await ide.saveFile(path);
		if (ok) {
			await ide.refreshGitStatus();
			ide.bumpGitRefresh();
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div
		class="flex items-center gap-2 border-b border-border px-3 py-1 text-micro text-muted-foreground"
	>
		<span class="min-w-0 flex-1 truncate" title={path}>{path}</span>
		{#if base.kind === 'commit'}
			<span class="shrink-0 font-mono">{base.parent.slice(0, 7)}…{base.commit.slice(0, 7)}</span>
		{:else if base.kind === 'staged'}
			<span class="shrink-0">{t('domain.git.staged')}</span>
		{:else}
			<span class="shrink-0">{t('page.ide.diffTabChangesSuffix')}</span>
		{/if}
		{#if !loading && !error && hunks.length > 0}
			<IconButton icon="copy" label={t('page.ide.copyDiff')} onclick={() => void copyDiffText()} />
		{/if}
		{#if base.kind !== 'commit'}
			<Button size="sm" variant="ghost" disabled={staging} onclick={() => void toggleStage()}>
				{stageLabel()}
			</Button>
		{/if}
		{#if onOpenInGit}
			<Button size="sm" variant="ghost" onclick={onOpenInGit}>
				{t('page.ide.openInGit')}
			</Button>
		{/if}
	</div>
	<div class="min-h-0 flex-1">
		{#if loading}
			<p class="p-4 text-caption text-muted-foreground">{t('common.loading')}</p>
		{:else if error}
			<div class="space-y-2 p-4">
				<p class="text-body text-destructive">{error}</p>
				<Button size="sm" variant="outline" onclick={() => void load()}>{t('common.retry')}</Button>
			</div>
		{:else if originalMissing && base.kind === 'unstaged'}
			<div class="space-y-2 p-4">
				<p class="text-body font-medium">{t('page.ide.diffNewFileTitle')}</p>
				<p class="text-caption text-muted-foreground">{t('page.ide.diffNewFileDesc')}</p>
				<Button size="sm" variant="outline" onclick={() => void ide.openFile(path, true)}>
					{t('page.ide.menuOpen')}
				</Button>
			</div>
		{:else if fullText}
			<MonacoDiffEditor
				{diffKey}
				filePath={editable ? path : null}
				{originalText}
				modifiedText={editable
					? (ide.tabs.find((entry) => entry.path === path)?.content ?? modifiedText)
					: modifiedText}
				{language}
				{editable}
				onChange={(next) => ide.updateContent(path, next)}
				onSave={() => void saveFromDiff()}
			/>
		{:else if binary}
			<div class="space-y-2 p-4">
				<p class="text-body font-medium">{t('page.ide.binaryTitle')}</p>
				<p class="text-caption text-muted-foreground">{t('page.ide.binaryDesc')}</p>
			</div>
		{:else if hunks.length === 0}
			<p class="p-4 text-center text-caption text-muted-foreground">{t('page.ide.diffEmpty')}</p>
		{:else}
			<div class="h-full space-y-2 overflow-y-auto p-3">
				{#each hunks as file (file.path)}
					<DiffView
						lines={file.lines}
						binary={file.binary}
						truncated={file.truncated}
						title={file.path}
					/>
				{/each}
			</div>
		{/if}
	</div>
</div>
