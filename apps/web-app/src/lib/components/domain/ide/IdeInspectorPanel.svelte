<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { ide } from '#lib/stores/ide.svelte';
	import { statFs, type FsStat } from '#lib/services/fs';
	import type { GitBadgeLevel } from '#lib/utils/git-badges';
	import { formatBytes, formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	interface Props {
		onclosetab: (path: string) => void;
	}

	let { onclosetab }: Props = $props();

	const statCache = new SvelteMap<string, FsStat>();

	const active = $derived(ide.active());
	const badge = $derived<GitBadgeLevel | null>(
		active && ide.gitBadgesAvailable ? (ide.gitBadges.get(active.path) ?? null) : null,
	);

	let stat = $state<FsStat | null>(null);
	let statLoading = $state(false);
	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	function badgeLabel(level: GitBadgeLevel): string {
		switch (level) {
			case 'added':
				return t('page.ide.badgeAdded');
			case 'modified':
				return t('page.ide.badgeModified');
			case 'deleted':
				return t('page.ide.badgeDeleted');
			case 'renamed':
				return t('page.ide.badgeRenamed');
			case 'conflicted':
				return t('page.ide.badgeConflicted');
			case 'untracked':
				return t('page.ide.badgeUntracked');
		}
	}

	function readOnlyReason(): string | null {
		if (!active) return null;
		if (active.truncated) return t('page.ide.readOnlyTruncated');
		if (active.isBinary) return t('page.ide.readOnlyBinary');
		return null;
	}

	$effect(() => {
		const current = active;
		if (!current) {
			stat = null;
			statLoading = false;
			return;
		}
		const key = `${current.path}@${current.hash ?? ''}`;
		const cached = statCache.get(key);
		if (cached) {
			stat = cached;
			statLoading = false;
			return;
		}
		let cancelled = false;
		statLoading = true;
		void statFs(current.path)
			.then((fresh) => {
				if (cancelled) return;
				statCache.set(key, fresh);
				stat = fresh;
			})
			.catch(() => {
				if (!cancelled) stat = null;
			})
			.finally(() => {
				if (!cancelled) statLoading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	async function copyPath(): Promise<void> {
		if (!active) return;
		try {
			await navigator.clipboard.writeText(active.path);
			copied = true;
			if (copyTimer) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => (copied = false), 1500);
		} catch {
			copied = false;
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col overflow-y-auto px-3 py-2">
	{#if !active}
		<div class="flex flex-1 items-center justify-center px-2 text-center">
			<div class="space-y-1">
				<p class="text-body font-medium">{t('page.ide.inspectorEmpty')}</p>
				<p class="text-caption text-muted-foreground">{t('page.ide.inspectorEmptyDesc')}</p>
			</div>
		</div>
	{:else}
		<section aria-label={t('page.ide.fileInfo')}>
			<h3 class="py-1 text-micro uppercase tracking-wide text-muted-foreground">
				{t('page.ide.fileInfo')}
			</h3>
			<dl class="space-y-1 text-caption">
				<div class="flex items-center justify-between gap-2">
					<dt class="shrink-0 text-muted-foreground">{t('page.ide.infoSize')}</dt>
					<dd class="truncate font-mono tabular-nums">
						{#if statLoading && !stat}
							{t('common.loading')}
						{:else}
							{formatBytes(stat?.size ?? active.content.length)}
						{/if}
					</dd>
				</div>
				<div class="flex items-center justify-between gap-2">
					<dt class="shrink-0 text-muted-foreground">{t('page.ide.infoModified')}</dt>
					<dd class="truncate font-mono tabular-nums">
						{#if stat?.modified_ms}
							{formatDateTime(stat.modified_ms)}
						{:else}
							—
						{/if}
					</dd>
				</div>
				<div class="flex items-center justify-between gap-2">
					<dt class="shrink-0 text-muted-foreground">{t('page.ide.infoHash')}</dt>
					<dd class="truncate font-mono tabular-nums" title={active.hash ?? ''}>
						{active.hash ? active.hash.slice(0, 12) : '—'}
					</dd>
				</div>
				<div class="flex items-center justify-between gap-2">
					<dt class="shrink-0 text-muted-foreground">{t('page.ide.infoLanguage')}</dt>
					<dd class="truncate font-mono">{active.language}</dd>
				</div>
				<div class="flex items-center justify-between gap-2">
					<dt class="shrink-0 text-muted-foreground">{t('page.ide.infoReadOnly')}</dt>
					<dd class="truncate">
						{#if readOnlyReason()}
							{readOnlyReason()}
						{:else}
							{t('page.ide.editable')}
						{/if}
					</dd>
				</div>
			</dl>
		</section>

		<section aria-label={t('page.ide.versionStatus')} class="mt-3 border-t border-border pt-2">
			<h3 class="py-1 text-micro uppercase tracking-wide text-muted-foreground">
				{t('page.ide.versionStatus')}
			</h3>
			{#if badge}
				<p class="text-caption">{badgeLabel(badge)}</p>
				{#if badge === 'conflicted'}
					<a href="/git" class="mt-1 inline-block text-caption text-info hover:underline">
						{t('page.ide.openGit')}
					</a>
				{/if}
			{:else}
				<p class="text-caption text-muted-foreground">{t('page.ide.noVersionChange')}</p>
			{/if}
		</section>

		<section aria-label={t('page.ide.quickActions')} class="mt-3 border-t border-border pt-2">
			<h3 class="py-1 text-micro uppercase tracking-wide text-muted-foreground">
				{t('page.ide.quickActions')}
			</h3>
			<div class="flex flex-col gap-1.5">
				<Button size="sm" variant="outline" onclick={() => void copyPath()}>
					{copied ? t('page.ide.copied') : t('page.ide.copyPath')}
				</Button>
				<Button size="sm" variant="outline" onclick={() => ide.requestReveal(active.path)}>
					{t('page.ide.locateInTree')}
				</Button>
				<Button size="sm" variant="ghost" onclick={() => onclosetab(active.path)}>
					{t('page.ide.closeTab')}
				</Button>
			</div>
		</section>
	{/if}
</div>
