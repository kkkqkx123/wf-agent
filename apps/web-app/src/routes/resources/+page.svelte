<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		modelProfiles,
		providers,
		scripts,
		skills,
		tools,
	} from '$lib/fixtures/resources';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	const TABS = [
		{ id: 'models', label: 'Models' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'scripts', label: 'Scripts' },
		{ id: 'skills', label: 'Skills' },
	];

	let tab = $state('models');

	let toolEnabled = $state(
		Object.fromEntries(tools.map((tool) => [tool.id, tool.enabled])),
	);
	let skillEnabled = $state(
		Object.fromEntries(skills.map((skill) => [skill.id, skill.enabled])),
	);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Models & tools"
		description="Profiles, providers, tool registry, scripts and skills."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => toasts.info('Refresh queued')}
			/>
			<Button size="sm" onclick={() => toasts.success('Creation form pending')}>
				<Icon name="plus" size={13} />
				New
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'models'}
			<div class="space-y-3">
				<Card title="Model profiles" bodyClass="p-0">
					<div class="overflow-x-auto">
						<table class="w-full border-collapse text-body">
							<thead>
								<tr class="border-b border-border">
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Profile</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Provider</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Model</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Status</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Requests</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Tokens</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Cost</th
									>
								</tr>
							</thead>
							<tbody>
								{#each modelProfiles as profile (profile.id)}
									<tr
										class="border-b border-border/60 transition-colors last:border-0 hover:bg-accent/40"
									>
										<td class="px-3 py-2.5">
											<span class="flex items-center gap-1.5">
												<span class="truncate">{profile.name}</span>
												{#if profile.isDefault}
													<Badge variant="info" class="text-[0.625rem]"
														>default</Badge
													>
												{/if}
											</span>
										</td>
										<td class="px-3 py-2.5 text-caption text-muted-foreground"
											>{profile.provider}</td
										>
										<td class="px-3 py-2.5 font-mono text-caption"
											>{profile.model}</td
										>
										<td class="px-3 py-2.5"
											><StatusBadge status={profile.status} size="sm" /></td
										>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption"
										>
											{formatNumber(profile.requests)}
										</td>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption text-muted-foreground"
										>
											{formatNumber(profile.tokens)}
										</td>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption"
										>
											{profile.cost === null
												? '—'
												: `$${profile.cost.toFixed(2)}`}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</Card>

				<Card title="Providers">
					<ul class="divide-y divide-border">
						{#each providers as provider (provider.id)}
							<li
								class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0"
							>
								<div class="min-w-0">
									<p class="truncate text-body">{provider.name}</p>
									<p
										class="truncate font-mono text-micro text-muted-foreground"
									>
										{provider.baseUrl}
									</p>
								</div>
								<div class="flex shrink-0 items-center gap-2">
									<span class="text-caption text-muted-foreground"
										>{provider.models} models</span
									>
									<StatusBadge status={provider.status} size="sm" />
								</div>
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		{:else if tab === 'tools'}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each tools as tool (tool.id)}
					<Card title={tool.name}>
						{#snippet actions()}
							<Switch
								checked={toolEnabled[tool.id] ?? false}
								label="Enable {tool.name}"
								hideLabel
								onchange={(checked) =>
									(toolEnabled = { ...toolEnabled, [tool.id]: checked })}
							/>
						{/snippet}
						<p class="text-caption text-muted-foreground">{tool.description}</p>
						<div
							class="mt-2 flex items-center justify-between text-micro text-muted-foreground"
						>
							<span class="rounded border border-border px-1.5 py-0.5"
								>{tool.kind}</span
							>
							<span class="tabular-nums">
								{formatNumber(tool.calls)} calls ·
								{tool.successRate === null
									? '—'
									: formatPercent(tool.successRate, 0)} ok
							</span>
						</div>
						{#snippet footer()}
							<div class="flex items-center gap-2">
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.info('Parameter check pending')}
								>
									Validate
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.info('Run pending')}
								>
									Run
								</Button>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{:else if tab === 'scripts'}
			<Card bodyClass="p-0">
				<div class="overflow-x-auto">
					<table class="w-full border-collapse text-body">
						<thead>
							<tr class="border-b border-border">
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Script</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Runtime</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>State</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Runs</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Updated</th
								>
							</tr>
						</thead>
						<tbody>
							{#each scripts as script (script.id)}
								<tr
									class="border-b border-border/60 transition-colors last:border-0 hover:bg-accent/40"
								>
									<td class="px-3 py-2.5 font-mono text-caption"
										>{script.name}</td
									>
									<td class="px-3 py-2.5 text-caption text-muted-foreground"
										>{script.runtime}</td
									>
									<td class="px-3 py-2.5">
										<StatusBadge
											status={script.enabled ? 'enabled' : 'disabled'}
											size="sm"
										/>
									</td>
									<td class="px-3 py-2.5 text-right tabular-nums text-caption">
										{formatNumber(script.runs)}
									</td>
									<td
										class="px-3 py-2.5 text-right text-caption text-muted-foreground"
									>
										{formatRelativeTime(script.updatedAt)}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</Card>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each skills as skill (skill.id)}
					<Card title={skill.name}>
						{#snippet actions()}
							<Switch
								checked={skillEnabled[skill.id] ?? false}
								label="Enable {skill.name}"
								hideLabel
								onchange={(checked) =>
									(skillEnabled = { ...skillEnabled, [skill.id]: checked })}
							/>
						{/snippet}
						<p class="text-caption text-muted-foreground">
							{skill.description}
						</p>
						<p
							class="mt-2 rounded-md bg-muted px-2 py-1.5 font-mono text-micro text-muted-foreground"
						>
							{skill.promptPreview}
						</p>
						{#snippet footer()}
							<div class="flex items-center justify-between">
								<span
									class={cn(
										'tabular-nums',
										skill.enabled ? 'text-success' : 'text-muted-foreground',
									)}
								>
									v{skill.version}
								</span>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.info('Prompt preview')}
								>
									View prompt
								</Button>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>
