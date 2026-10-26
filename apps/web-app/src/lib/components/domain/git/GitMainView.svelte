<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import DiffView from '#lib/ui/components/DiffView.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<TuiPanel
	title="Main"
	description={workbench.selection
		? t('domain.git.selected', { selection: workbench.selection })
		: t('domain.git.mainViewDescription')}
	focused={workbench.panel === 'commits' || workbench.panel === 'files'}
>
	{#if workbench.panel === 'commits' && workbench.commitFiles.length > 0}
		<ul class="mb-2 divide-y divide-border">
			{#each workbench.commitFiles as file (file.path)}
				<li>
					<button
						type="button"
						onclick={() =>
							workbench.selection &&
							void workbench.showCommitFileDiff(workbench.selection as string, file.path)}
						class="flex w-full items-center justify-between gap-2 px-1 py-1 text-left hover:bg-accent/50"
					>
						<span class="truncate font-mono text-caption">{file.path}</span>
						<span class="font-mono text-micro text-muted-foreground">{file.status}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	{#if workbench.diff.length === 0}
		<p class="text-caption text-muted-foreground">
			{workbench.panel === 'commits' ? t('domain.git.pickCommitFile') : t('domain.git.pickFile')}
		</p>
	{:else}
		<p class="mb-1 text-micro text-muted-foreground">
			{workbench.diffSource === 'commit'
				? t('domain.git.commitRangeDiff')
				: t('domain.git.unstagedDiff')}
		</p>
		{#each workbench.diff as file (file.path)}
			<div class="mb-2">
				{#if workbench.diffSource === 'unstaged'}
					{@const hunkCount = file.lines.filter((l) => l.type === 'meta').length}
					{#if hunkCount > 0}
						<div class="mb-1 flex flex-wrap gap-1">
							{#each Array.from({ length: hunkCount }, (_, i) => i) as i (i)}
								<Button
									size="sm"
									variant="outline"
									disabled={gitStore.busy}
									onclick={() => void workbench.stageHunk(file.path, i)}
								>
									{t('domain.git.stageHunk', { number: i + 1 })}
								</Button>
							{/each}
						</div>
					{/if}
				{/if}
				<DiffView
					lines={file.lines}
					title={file.truncated
						? `${file.path} (+${file.additions}/-${file.deletions}) · truncated`
						: `${file.path} (+${file.additions}/-${file.deletions})`}
				/>
			</div>
		{/each}
	{/if}
	{#snippet footer()}
		<div class="flex flex-wrap gap-x-3 gap-y-1">
			{#each workbench.controls as control (control.id)}
				{@const spec = workbench.specFor(control.id)}
				<Button
					size="sm"
					variant="ghost"
					disabled={gitStore.busy || (spec ? !spec.available : false)}
					title={spec?.reason ?? `${control.name} (${control.hint})`}
					onclick={() => void workbench.runActionId(control.id)}
				>
					<span class="font-mono">[{control.hint}]</span>
					{control.name}
				</Button>
			{/each}
		</div>
	{/snippet}
</TuiPanel>
