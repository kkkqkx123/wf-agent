<script lang="ts">
	import { browser } from '$app/env';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		executionId: string;
		class?: string;
	}

	let { executionId, class: className = '' }: Props = $props();

	type Vote = 'up' | 'down' | 'none';

	function storageKey(id: string): string {
		return `wf-execution-annotation:${id}`;
	}

	function readStored(id: string): { vote: Vote; note: string } {
		if (!browser) return { vote: 'none', note: '' };
		try {
			const raw = localStorage.getItem(storageKey(id));
			if (!raw) return { vote: 'none', note: '' };
			const parsed = JSON.parse(raw) as { vote?: unknown; note?: unknown };
			return {
				vote:
					parsed.vote === 'up' || parsed.vote === 'down' ? parsed.vote : 'none',
				note: typeof parsed.note === 'string' ? parsed.note : '',
			};
		} catch {
			return { vote: 'none', note: '' };
		}
	}

	let vote = $state<Vote>('none');
	let note = $state('');
	let saved = $state(true);

	$effect(() => {
		const fresh = readStored(executionId);
		vote = fresh.vote;
		note = fresh.note;
		saved = true;
	});

	function persist(nextVote: Vote, nextNote: string): void {
		if (!browser) return;
		try {
			localStorage.setItem(
				storageKey(executionId),
				JSON.stringify({ vote: nextVote, note: nextNote }),
			);
		} catch {
			// Private mode or quota errors keep the in-memory value only.
		}
		saved = true;
	}

	function setVote(next: Vote): void {
		vote = vote === next ? 'none' : next;
		saved = false;
		persist(vote, note);
	}

	function saveNote(): void {
		saved = false;
		persist(vote, note);
	}

	function clear(): void {
		vote = 'none';
		note = '';
		persist('none', '');
	}
</script>

<Card
	title="Annotation"
	description="Local only; no backend field is read or written."
	class={className}
>
	{#snippet actions()}
		<span class="flex items-center gap-1">
			<button
				type="button"
				onclick={() => setVote('up')}
				aria-pressed={vote === 'up'}
				title="Mark helpful"
				class={cn(
					'rounded-md border p-1.5 transition-colors',
					vote === 'up'
						? 'border-success/50 bg-success/10 text-success'
						: 'border-border text-muted-foreground hover:bg-accent',
				)}
			>
				<Icon name="check" size={13} />
			</button>
			<button
				type="button"
				onclick={() => setVote('down')}
				aria-pressed={vote === 'down'}
				title="Mark unhelpful"
				class={cn(
					'rounded-md border p-1.5 transition-colors',
					vote === 'down'
						? 'border-destructive/50 bg-destructive/10 text-destructive'
						: 'border-border text-muted-foreground hover:bg-accent',
				)}
			>
				<Icon name="x" size={13} />
			</button>
		</span>
	{/snippet}
	<Textarea
		bind:value={note}
		placeholder="Add a local note about this run…"
		class="min-h-16 text-caption"
	/>
	<div class="mt-2 flex items-center justify-between gap-2">
		<span class="text-micro text-muted-foreground">
			{saved ? 'Saved in this browser' : 'Saving…'}
		</span>
		<span class="flex items-center gap-1.5">
			<Button variant="ghost" size="sm" onclick={clear}>Clear</Button>
			<Button variant="outline" size="sm" onclick={saveNote}>Save</Button>
		</span>
	</div>
</Card>
