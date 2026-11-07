<script lang="ts">
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { CONFIRM_DANGERS } from '#lib/git/workbench.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let hintControls = $derived(workbench.controls.slice(0, 3));
	let expanded = $state(false);
	let shownControls = $derived(expanded ? workbench.controls : workbench.controls.slice(0, 4));
	let hiddenCount = $derived(workbench.controls.length - shownControls.length);
	let candidates = $derived(
		(workbench.tokenResult.kind === 'prefix' || workbench.tokenResult.kind === 'help'
			? (workbench.tokenResult.candidates ?? [])
			: []
		).slice(0, 8),
	);

	$effect(() => {
		// Collapse the control strip when switching panels; the default stays at four.
		void workbench.panel;
		expanded = false;
	});
</script>

<TuiPanel title={t('domain.git.keyInputTitle')} description={t('domain.git.keyInputDescription')}>
	<div class="flex flex-col gap-2 sm:flex-row sm:items-end">
		<span
			class="shrink-0 rounded-sm bg-accent px-1.5 py-1 font-mono text-micro font-semibold tracking-wider text-info uppercase"
			aria-label={t('domain.git.activePanel')}
		>
			[{workbench.panel}]
		</span>
		<div class="min-w-0 flex-1">
			<Input
				id="git-input"
				label={t('domain.git.keyLabel')}
				size="sm"
				bind:value={workbench.token}
				placeholder={t('common.gitShortcutHelp')}
				onkeydown={(event) => {
					if (event.key === 'Enter') void workbench.submitToken();
					else if (event.key === 'Escape') {
						// An empty key input hands focus back to the board so
						// single-key shortcuts work again without clicking.
						if (workbench.token === '') (event.target as HTMLInputElement | null)?.blur?.();
						else workbench.token = '';
					}
				}}
			/>
		</div>
		<Button size="sm" disabled={gitStore.busy} onclick={() => void workbench.submitToken()}
			>{t('domain.git.run')}</Button
		>
		<Button size="sm" variant="ghost" onclick={() => (workbench.helpOpen = true)}>
			{t('domain.git.pressForAll')}
		</Button>
	</div>
	{#if workbench.selection}
		<p class="mt-1 truncate font-mono text-micro text-muted-foreground">
			{t('domain.git.selected', { selection: workbench.selection })}
		</p>
	{/if}
	{#if workbench.tokenResult.kind === 'action'}
		<p class="mt-1 font-mono text-micro text-muted-foreground">
			{t('domain.git.willRun')}
			<span class="font-mono">{workbench.tokenResult.action}</span>
			{#if workbench.tokenResult.danger !== 'readonly' && workbench.tokenResult.danger !== 'recoverable'}
				<Badge variant="danger">{workbench.tokenResult.danger}</Badge>
			{/if}
		</p>
	{:else if candidates.length > 0}
		<div
			class="mt-1 flex flex-wrap gap-1"
			role="group"
			aria-label={t('domain.git.matchingActions')}
		>
			{#each candidates as candidate (candidate)}
				<Button
					size="sm"
					variant="outline"
					disabled={gitStore.busy}
					title={candidate}
					onclick={() => void workbench.runActionId(candidate)}
				>
					{workbench.controls.find((entry) => entry.id === candidate)?.name ?? candidate}
					<span class="text-muted-foreground">· {candidate}</span>
				</Button>
			{/each}
		</div>
	{:else if workbench.tokenResult.kind === 'empty' && hintControls.length > 0}
		<p class="mt-1 truncate font-mono text-micro text-muted-foreground">
			{hintControls.map((entry) => `${entry.hint} ${entry.name}`).join(' · ')}
		</p>
	{/if}
	{#if shownControls.length > 0}
		<div
			class="mt-2 flex flex-nowrap items-center gap-1 overflow-x-auto border-t border-border pt-2"
			role="group"
			aria-label={t('domain.git.panelControls')}
		>
			{#each shownControls as control (control.id)}
				<Button
					size="sm"
					variant={control.id === shownControls[0].id ? 'outline' : 'ghost'}
					disabled={gitStore.busy}
					title={control.legacyKey
						? `${control.name} (${control.hint} · ${control.legacyKey})`
						: `${control.name} (${control.hint})`}
					onclick={() => void workbench.runActionId(control.id)}
					class={CONFIRM_DANGERS.has(control.danger) ? 'text-destructive' : ''}
				>
					<span class="text-muted-foreground">{control.hint}</span>
					{control.name}
				</Button>
			{/each}
			{#if hiddenCount > 0 || expanded}
				<Button
					size="sm"
					variant="ghost"
					onclick={() => (expanded = !expanded)}
					title={expanded ? t('domain.git.showFewerControls') : t('domain.git.showMoreControls')}
				>
					{expanded
						? t('domain.git.showFewerControls')
						: `${t('domain.git.showMoreControls')} (${hiddenCount})`}
				</Button>
			{/if}
		</div>
	{/if}
	{#if gitStore.actionError}
		<p class="mt-1 font-mono text-micro text-destructive">
			{gitStore.actionError}
		</p>
	{/if}
</TuiPanel>
