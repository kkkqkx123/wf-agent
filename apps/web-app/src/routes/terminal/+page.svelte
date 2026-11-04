<script lang="ts">
	import { onMount } from 'svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import { getMeta } from '#lib/services/git';
	import { t } from '#lib/i18n';

	let loading = $state(true);
	let root = $state('ide');

	onMount(() => {
		void getMeta()
			.then((meta) => {
				root = meta.root || 'ide';
			})
			.catch(() => {
				root = 'ide';
			})
			.finally(() => {
				loading = false;
			});
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.terminal.title')} description={t('page.terminal.description')} />
	<div class="min-h-0 flex-1 px-4 py-3">
		<PageState {loading}>
			<div class="h-full min-h-0">
				<PtyPanel
				repo={root}
				cwd={root === 'ide' ? undefined : root}
				fill
				open
				quickCommands={[{ label: 'lazygit', command: 'lazygit' }]}
			/>
			</div>
		</PageState>
	</div>
</div>
