<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import KeyValueList from '#lib/components/domain/KeyValueList.svelte';
	import {
		summarizeTemplate,
		type TemplateDetail,
	} from '#lib/services/templates';
	import type { TemplateKind } from '#lib/types/models';
	import type { TemplateEditController } from '#lib/templates/template-edit-session.svelte';
	import { formatNumber } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	interface Props {
		kind: TemplateKind;
		detail: TemplateDetail;
		controller: TemplateEditController;
	}

	let { kind, detail, controller }: Props = $props();

	const summary = $derived(summarizeTemplate(kind, detail.raw));
</script>

<div class="flex flex-wrap items-center gap-1.5 text-caption">
	<StatusBadge status="active" size="sm" dot={false} />
	<span class="text-muted-foreground">{detail.template.kind}</span>
	<span class="text-muted-foreground">·</span>
	<span class="text-muted-foreground">
		{t('domain.template.version', { version: detail.version ?? '—' })}
	</span>
	<span class="text-muted-foreground">·</span>
	<span class="text-muted-foreground">
		{t('domain.template.uses', { count: formatNumber(detail.template.usage) })}
	</span>
</div>
{#if detail.template.description}
	<p class="text-caption text-muted-foreground">
		{detail.template.description}
	</p>
{/if}
{#if summary.length > 0}
	<div>
		<span class="text-caption font-medium">{t('domain.template.summary')}</span>
		<KeyValueList items={summary} dense />
	</div>
{/if}
<div>
	<span class="text-caption font-medium"
		>{t('domain.template.definitionPreview')}</span
	>
	<pre
		class="mt-1 max-h-72 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{detail.definitionJson}</pre>
</div>
<div class="flex flex-wrap items-center gap-2">
	<Button
		variant="outline"
		size="sm"
		onclick={() => controller.requestView('edit')}
	>
		<Icon name="pencil" size={13} />
		{t('common.edit')}
	</Button>
	<Button
		variant="outline"
		size="sm"
		onclick={() => void controller.runExport()}
	>
		<Icon name="download" size={13} />
		{t('domain.template.export')}
	</Button>
	<Button
		variant="outline"
		size="sm"
		onclick={() => void controller.runClone()}
	>
		{t('domain.template.cloneAsNew')}
	</Button>
	{#if kind === 'workflow'}
		<Button
			variant="outline"
			size="sm"
			onclick={() => controller.requestView('graph')}
		>
			{t('domain.template.editGraph')}
		</Button>
	{/if}
	{#if controller.deleteArmed}
		<Button size="sm" onclick={() => void controller.runDelete()}>
			{kind === 'workflow'
				? t('domain.template.confirmRemoveFromLibrary')
				: t('domain.template.confirmDelete')}
		</Button>
		<Button
			variant="ghost"
			size="sm"
			onclick={() => (controller.deleteArmed = false)}
		>
			{t('domain.template.keep')}
		</Button>
	{:else}
		<Button
			variant="ghost"
			size="sm"
			onclick={() => (controller.deleteArmed = true)}
		>
			{kind === 'workflow'
				? t('domain.template.removeFromLibrary')
				: t('common.delete')}
		</Button>
	{/if}
</div>
