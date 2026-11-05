<script module lang="ts">
	import type { Snippet } from 'svelte';

	/** One colored short prefix segment before the row label. */
	export interface BoardMarker {
		text: string;
		class?: string;
	}

	/** Normalized row model shared by every board list panel. */
	export interface BoardRow {
		id: string;
		label: string;
		/** Tone class for the label text. */
		labelClass?: string;
		/** Short fixed-width prefix segments (status markers, short ids). */
		markers?: BoardMarker[];
		/** Trailing muted detail text (url, commit, selector, branch). */
		detail?: string;
		/** Trailing badge text (refs, ahead/behind counts). */
		badge?: string;
	}
</script>

<script lang="ts">
	import BoardFilter from '#lib/components/domain/git/BoardFilter.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import type { GitWorkbench, PanelId } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
		panel: PanelId;
		title: string;
		rows: BoardRow[];
		selectedId: string | null;
		emptyText: string;
		/** DOM id of the built-in filter input (`git-filter-<panel>`). */
		filterId?: string;
		/** Two-way bound filter query for the built-in filter input. */
		filterValue?: string;
		/** Rendered below the list and empty state (footer, load-more). */
		footer?: Snippet;
		/** Optional leading decoration per row (commit graph lane). */
		leading?: Snippet<[BoardRow]>;
	}

	let {
		workbench,
		panel,
		title,
		rows,
		selectedId,
		emptyText,
		filterId,
		filterValue = $bindable(''),
		footer,
		leading,
	}: Props = $props();
</script>

<TuiPanel {title} count={rows.length} focused={workbench.panel === panel}>
	{#if filterId}
		<BoardFilter
			id={filterId}
			label={t('domain.git.filter')}
			placeholder={t('domain.git.filterPlaceholder')}
			bind:value={filterValue}
		/>
	{/if}
	<ul class="max-h-[32vh] overflow-y-auto">
		{#each rows as row (row.id)}
			<li>
				<button
					type="button"
					data-board-row
					data-board-panel={panel}
					data-board-id={row.id}
					onclick={() => void workbench.selectRow(panel, row.id)}
					aria-pressed={selectedId === row.id}
					class="flex w-full items-center gap-1.5 px-1 py-0.5 text-left font-mono text-micro hover:bg-accent/50 {selectedId ===
					row.id
						? 'bg-accent/70'
						: ''}"
				>
					{#if leading}{@render leading(row)}{/if}
					{#if row.markers}
						<span class="shrink-0">
							{#each row.markers as marker, index (index)}
								<span class={marker.class ?? 'text-muted-foreground'}>{marker.text}</span>
							{/each}
						</span>
					{/if}
					<span class="min-w-0 flex-1 truncate {row.labelClass ?? ''}">{row.label}</span>
					{#if row.detail}
						<span class="max-w-40 shrink-0 truncate text-muted-foreground">{row.detail}</span>
					{/if}
					{#if row.badge}
						<span class="max-w-28 shrink-0 truncate text-muted-foreground">{row.badge}</span>
					{/if}
				</button>
			</li>
		{/each}
	</ul>
	{#if rows.length === 0}
		<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">{emptyText}</p>
	{/if}
	{#if footer}{@render footer()}{/if}
</TuiPanel>
