<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { bindingsFor } from '#lib/git/bindings';
	import { GUARDED_DANGERS, PARAM_LABELS } from '#lib/git/workbench.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<Dialog
	bind:open={workbench.dialogOpen}
	title={t('domain.git.actionTitle')}
	description={t('domain.git.actionDescription')}
>
	{#if workbench.dialogAction}
		<p class="mb-2 font-mono text-caption">{workbench.dialogAction}</p>
		{#each workbench.dialogParams as param (param)}
			<Input
				label={PARAM_LABELS[param] ?? param}
				size="sm"
				bind:value={workbench.dialogValues[param]}
				placeholder={PARAM_LABELS[param] ?? param}
			/>
		{/each}
		{#if workbench.dialogNeedsConfirm}
			<div class="mt-2">
				<Input
					label={t('domain.git.confirmTarget', {
						target: workbench.dialogTarget || 'target',
					})}
					size="sm"
					bind:value={workbench.dialogConfirm}
					placeholder={workbench.dialogTarget || 'target'}
				/>
				{#if workbench.dialogConfirm.length > 0 && !workbench.dialogConfirmMatches}
					<p class="mt-1 text-micro text-destructive">
						{t('domain.git.confirmMismatch')}
					</p>
				{/if}
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button
			variant="ghost"
			size="sm"
			onclick={() => {
				workbench.dialogOpen = false;
				workbench.dialogAction = null;
			}}>{t('common.cancel')}</Button
		>
		<Button
			size="sm"
			disabled={gitStore.busy || !workbench.dialogCanSubmit}
			onclick={() => void workbench.confirmDialog()}>{t('common.confirm')}</Button
		>
	{/snippet}
</Dialog>

<Dialog
	bind:open={workbench.helpOpen}
	title={t('domain.git.actionsTitle')}
	description={t('domain.git.actionsDescription', { panel: workbench.panel })}
>
	<div class="space-y-1">
		{#each bindingsFor(workbench.panel) as binding (binding.id)}
			<button
				type="button"
				onclick={() => {
					workbench.helpOpen = false;
					void workbench.runActionId(binding.id);
				}}
				class="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left text-caption hover:bg-accent/50"
			>
				<span
					>{binding.name}
					<span class="font-mono text-micro text-muted-foreground"
						>{binding.id}</span
					></span
				>
				<span class="flex items-center gap-2">
					{#if GUARDED_DANGERS.has(binding.danger)}
						<Badge variant="danger">{binding.danger}</Badge>
					{:else}
						<Badge variant="neutral">{binding.danger}</Badge>
					{/if}
					<span class="font-mono text-micro text-muted-foreground"
						>{binding.hint}</span
					>
				</span>
			</button>
		{/each}
	</div>
</Dialog>
