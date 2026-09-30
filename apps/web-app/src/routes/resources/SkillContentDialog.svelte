<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { getSkillContent } from '$lib/services/resources';
	import { toasts } from '$lib/stores/toast.svelte';
	import type { Skill } from '$lib/types/models';

	interface Props {
		open?: boolean;
		skill?: Skill | null;
	}

	let { open = $bindable(false), skill = null }: Props = $props();

	let skillContent = $state('');
	let skillContentError = $state<string | null>(null);
	let skillFilter = $state('');
	let loadedSkillId = $state('');

	const skillFilteredContent = $derived(
		skillFilter.trim()
			? skillContent
					.split('\n')
					.filter((line) =>
						line.toLowerCase().includes(skillFilter.trim().toLowerCase()),
					)
					.join('\n') || 'No lines match the filter.'
			: skillContent,
	);

	$effect(() => {
		const id = open && skill ? skill.id : '';
		if (id && id !== loadedSkillId) {
			loadedSkillId = id;
			skillContent = '';
			skillContentError = null;
			skillFilter = '';
			void loadSkillContent(skill);
		} else if (!id) {
			loadedSkillId = '';
		}
	});

	async function loadSkillContent(next: Skill | null): Promise<void> {
		if (!next) return;
		try {
			skillContent = await getSkillContent(next.name);
		} catch (e) {
			skillContentError =
				e instanceof Error ? e.message : 'Prompt failed to load.';
		}
	}

	async function copySkillContent(): Promise<void> {
		if (!skillContent) return;
		try {
			await navigator.clipboard.writeText(skillContent);
			toasts.success('Prompt copied');
		} catch (e) {
			toasts.error('Copy failed', e instanceof Error ? e.message : undefined);
		}
	}
</script>

<Dialog bind:open title={skill ? `Prompt · ${skill.name}` : 'Skill prompt'}>
	{#if skillContentError}
		<ErrorState
			title="Prompt failed to load"
			description={skillContentError}
			onretry={() => {
				if (skill) void loadSkillContent(skill);
			}}
		/>
	{:else if !skillContent}
		<Skeleton lines={6} />
	{:else}
		<div class="mb-2 flex items-center gap-2">
			<Input
				bind:value={skillFilter}
				placeholder="Filter lines…"
				size="sm"
				class="w-full"
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => void copySkillContent()}
			>
				Copy
			</Button>
		</div>
		<pre
			class="max-h-96 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-small">{skillFilteredContent}</pre>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end">
			<Button variant="ghost" size="sm" onclick={() => (open = false)}>
				Close
			</Button>
		</div>
	{/snippet}
</Dialog>
