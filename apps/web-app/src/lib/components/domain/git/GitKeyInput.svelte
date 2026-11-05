<script lang="ts">
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { GUARDED_DANGERS } from '#lib/git/workbench.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let hintControls = $derived(workbench.controls.slice(0, 3));
	let stripControls = $derived(workbench.controls.slice(0, 4));
	let candidates = $derived(
		(workbench.tokenResult.kind === 'prefix' || workbench.tokenResult.kind === 'help'
			? (workbench.tokenResult.candidates ?? [])
			: []
		).slice(0, 8),
	);
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
					else if (event.key === 'Escape') workbench.token = '';
				}}
			/>
		</div>
		<Button size="sm" disabled={gitStore.busy} onclick={() => void workbench.submitToken()}
			>{t('domain.git.run')}</Button
		>
	</div>
	{#if workbench.selection}
		<p class="mt-1 truncate font-mono text-micro text-muted-foreground">
			{t('domain.git.selected', { selection: workbench.selection })}
		</p>
	{/if}
	{#if workbench.tokenResult.kind === 'action'}
		<p class="mt-1 text-caption">
			{t('domain.git.willRun')}
			<span class="font-mono">{workbench.tokenResult.action}</span>
			{#if workbench.tokenResult.danger !== 'readonly' && workbench.tokenResult.danger !== 'recoverable'}
				<Badge variant="danger">{workbench.tokenResult.danger}</Badge>
			{/if}
		</p>
	{:else if candidates.length > 0}
		<div class="mt-1 flex flex-wrap gap-1" role="group" aria-label={t('domain.git.matchingActions')}>
			{#each candidates as candidate (candidate)}
				<button
					type="button"
					disabled={gitStore.busy}
					title={candidate}
					onclick={() => void workbench.runActionId(candidate)}
					class="shrink-0 rounded-sm border border-border px-1.5 py-0.5 font-mono text-micro text-foreground hover:bg-accent disabled:opacity-50"
				>
					{workbench.controls.find((entry) => entry.id === candidate)?.name ?? candidate}
					<span class="text-muted-foreground">· {candidate}</span>
				</button>
			{/each}
		</div>
	{:else if workbench.tokenResult.kind === 'empty' && hintControls.length > 0}
		<p class="mt-1 truncate font-mono text-micro text-muted-foreground">
			{hintControls.map((entry) => `${entry.hint} ${entry.name}`).join(' · ')}
		</p>
	{/if}
	{#if stripControls.length > 0}
		<div
			class="mt-2 flex flex-nowrap items-center gap-1 overflow-x-auto border-t border-border pt-2"
			role="group"
			aria-label={t('domain.git.panelControls')}
		>
			{#each stripControls as control, index (control.id)}
				{#if index === 0}
					<Button
						size="sm"
						variant="outline"
						disabled={gitStore.busy}
						title={control.legacyKey
							? `${control.name} (${control.hint} · ${control.legacyKey})`
							: `${control.name} (${control.hint})`}
						onclick={() => void workbench.runActionId(control.id)}
					>
						{control.name}
					</Button>
				{:else}
					<button
						type="button"
						disabled={gitStore.busy}
						title={control.legacyKey
							? `${control.name} (${control.hint} · ${control.legacyKey})`
							: `${control.name} (${control.hint})`}
						onclick={() => void workbench.runActionId(control.id)}
						class="shrink-0 rounded-sm px-1.5 py-0.5 font-mono text-micro hover:bg-accent disabled:opacity-50 {GUARDED_DANGERS.has(
							control.danger,
						)
							? 'text-destructive'
							: 'text-foreground'}"
					>
						<span class="text-muted-foreground">{control.hint}</span>
						{control.name}
					</button>
				{/if}
			{/each}
		</div>
	{/if}
	{#if gitStore.actionError}
		<p class="mt-1 font-mono text-micro text-destructive">
			{gitStore.actionError}
		</p>
	{/if}
</TuiPanel>
