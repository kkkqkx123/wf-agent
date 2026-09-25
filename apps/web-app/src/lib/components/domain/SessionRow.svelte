<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import type { AgentLoop } from '$lib/types/models';
	import { formatRelativeTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		session: AgentLoop;
		label: string;
		selected?: boolean;
		starred?: boolean;
		/** Marks a session with unsent composer text. */
		draft?: boolean;
		onselect?: (id: string) => void;
		onstar?: (id: string) => void;
	}

	let {
		session,
		label,
		selected = false,
		starred = false,
		draft = false,
		onselect,
		onstar,
	}: Props = $props();
</script>

<li
	class={cn(
		'group/session flex items-center gap-0.5 rounded-md transition-colors',
		selected ? 'bg-sidebar-accent' : 'hover:bg-sidebar-accent/60',
	)}
>
	<button
		type="button"
		aria-label={starred ? 'Unstar session' : 'Star session'}
		aria-pressed={starred}
		class={cn(
			'shrink-0 rounded p-1.5 transition-colors hover:bg-accent',
			starred
				? 'text-warning'
				: 'text-muted-foreground/40 group-hover/session:text-muted-foreground',
		)}
		onclick={() => onstar?.(session.id)}
	>
		<Icon name="star" size={12} />
	</button>
	<button
		type="button"
		onclick={() => onselect?.(session.id)}
		aria-current={selected}
		class="flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pr-1.5 text-left"
	>
		<span class="min-w-0 flex-1">
			<span
				class={cn(
					'block truncate text-body',
					selected
						? 'font-medium text-foreground'
						: 'text-sidebar-foreground/90',
				)}
			>
				{label}
			</span>
			<span
				class="mt-0.5 flex items-center gap-1.5 truncate text-micro text-muted-foreground"
			>
				{#if draft}
					<Icon name="pencil" size={10} class="shrink-0 text-warning" />
				{/if}
				<span class="truncate">{session.profileId ?? '—'}</span>
				<span class="shrink-0">{formatRelativeTime(session.startedAt)}</span>
			</span>
		</span>
		<StatusBadge status={session.status} size="sm" dot={false} />
	</button>
</li>
