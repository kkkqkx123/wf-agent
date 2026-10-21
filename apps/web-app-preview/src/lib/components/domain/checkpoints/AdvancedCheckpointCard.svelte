<script lang="ts">
	import Card from '@wf-agent/ui/components/Card.svelte';
	import TimeRangeQuery from '#lib/components/domain/checkpoints/TimeRangeQuery.svelte';
	import EntityBatchQuery from '#lib/components/domain/checkpoints/EntityBatchQuery.svelte';
	import EntityMetadata from '#lib/components/domain/checkpoints/EntityMetadata.svelte';
	import AgentCheckpointStats from '#lib/components/domain/checkpoints/AgentCheckpointStats.svelte';
	import { t } from '#lib/i18n';
</script>

<Card
	title={t('domain.checkpoint.advancedTitle')}
	description={t('domain.checkpoint.advancedDescription')}
>
	<div class="grid gap-4 xl:grid-cols-2">
		<TimeRangeQuery />
		<EntityBatchQuery />
		<EntityMetadata />
		<AgentCheckpointStats />
	</div>
</Card>
