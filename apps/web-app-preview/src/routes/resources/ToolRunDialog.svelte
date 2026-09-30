<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import IssueList from '$lib/components/domain/IssueList.svelte';
	import JsonEditor from '@wf-agent/ui/components/JsonEditor.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import { jsonErrorLine, type TemplateIssue } from '$lib/services/templates';
	import {
		executeTool,
		formFromToolParams,
		formToToolParams,
		getToolDetail,
		validateToolParams,
		type ToolParameterSchema,
	} from '$lib/services/resources';
	import type { Tool, ToolRun } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';

	interface Props {
		open?: boolean;
		tool?: Tool | null;
	}

	let { open = $bindable(false), tool = null }: Props = $props();

	let toolParams = $state('{}');
	let toolSyntaxError = $state<string | null>(null);
	let toolEditor = $state<{
		scrollToLine: (line: number) => void;
	} | null>(null);
	let toolIssues = $state<string[]>([]);
	let toolValidated = $state(false);
	let toolValidatedSnapshot = $state<string | null>(null);
	let toolRun = $state<ToolRun | null>(null);
	let toolBusy = $state(false);
	let toolTab = $state<'form' | 'json'>('json');
	let toolSchema = $state<ToolParameterSchema | null>(null);
	let toolForm = $state<Record<string, string>>({});
	let loadedToolId = $state('');

	const toolErrorLine = $derived(
		toolSyntaxError ? jsonErrorLine(toolParams, toolSyntaxError) : null,
	);
	const toolValidationStale = $derived(
		toolValidated &&
			toolValidatedSnapshot !== null &&
			toolValidatedSnapshot !== toolParams,
	);
	const toolShowValid = $derived(toolValidated && !toolValidationStale);
	const toolTemplateIssues = $derived<TemplateIssue[]>(
		toolIssues.map((message) => ({
			source: 'server',
			field: null,
			message,
			nodeId: null,
		})),
	);

	$effect(() => {
		const id = open && tool ? tool.id : '';
		if (id && id !== loadedToolId) {
			loadedToolId = id;
			resetDialog();
			void loadToolSchema(id);
		} else if (!id) {
			loadedToolId = '';
		}
	});

	function resetDialog(): void {
		toolParams = '{}';
		toolSyntaxError = null;
		toolIssues = [];
		toolValidated = false;
		toolValidatedSnapshot = null;
		toolRun = null;
		toolTab = 'json';
		toolSchema = null;
		toolForm = {};
	}

	async function loadToolSchema(toolId: string): Promise<void> {
		try {
			const detail = await getToolDetail(toolId);
			toolSchema = detail.parameters;
			if (toolSchema && Object.keys(toolSchema.properties).length > 0) {
				toolForm = formFromToolParams(toolSchema, {});
				toolTab = 'form';
			}
		} catch {
			toolSchema = null;
		}
	}

	function enterToolForm(): void {
		if (!toolSchema) return;
		try {
			const parsed = JSON.parse(toolParams) as Record<string, unknown>;
			toolForm = formFromToolParams(
				toolSchema,
				parsed && typeof parsed === 'object' ? parsed : {},
			);
		} catch {
			toolForm = formFromToolParams(toolSchema, {});
		}
		toolTab = 'form';
	}

	function syncToolFormToText(): void {
		if (!toolSchema) return;
		let base: Record<string, unknown> = {};
		try {
			const parsed = JSON.parse(toolParams) as unknown;
			if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
				base = parsed as Record<string, unknown>;
			}
		} catch {
			base = {};
		}
		toolParams = JSON.stringify(
			formToToolParams(toolSchema, toolForm, base),
			null,
			2,
		);
	}

	function switchToolTab(next: 'form' | 'json'): void {
		if (next === 'form' && toolTab !== 'form') enterToolForm();
		if (next === 'json' && toolTab !== 'json') syncToolFormToText();
		toolTab = next;
	}

	function parseToolParams(): {
		value: Record<string, unknown> | null;
		error: string | null;
	} {
		try {
			const value = JSON.parse(toolParams) as unknown;
			if (!value || typeof value !== 'object' || Array.isArray(value)) {
				return { value: null, error: 'Parameters must be a JSON object' };
			}
			return { value: value as Record<string, unknown>, error: null };
		} catch (e) {
			return {
				value: null,
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
	}

	async function runToolValidation(): Promise<void> {
		if (!tool) return;
		if (toolTab === 'form') syncToolFormToText();
		const { value, error } = parseToolParams();
		toolSyntaxError = error;
		if (!value) {
			toolIssues = [error ?? 'Invalid JSON'];
			toolValidated = false;
			toolValidatedSnapshot = null;
			return;
		}
		toolBusy = true;
		try {
			toolIssues = await validateToolParams(tool.id, value);
			toolValidated = true;
			toolValidatedSnapshot = toolParams;
		} catch (e) {
			toolIssues = [e instanceof Error ? e.message : 'Validation failed.'];
			toolValidated = false;
			toolValidatedSnapshot = null;
		} finally {
			toolBusy = false;
		}
	}

	async function runToolExecute(): Promise<void> {
		if (!tool) return;
		if (toolTab === 'form') syncToolFormToText();
		const { value, error } = parseToolParams();
		toolSyntaxError = error;
		if (!value) {
			toolIssues = [error ?? 'Invalid JSON'];
			toolValidated = false;
			toolValidatedSnapshot = null;
			return;
		}
		toolBusy = true;
		toolRun = null;
		try {
			toolRun = await executeTool(tool.id, value);
		} catch (e) {
			toasts.error(
				'Tool run failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			toolBusy = false;
		}
	}
</script>

<Dialog
	bind:open
	title={tool ? `Validate / run ${tool.name}` : 'Tool'}
	description={toolSchema
		? 'Structured fields with JSON advanced mode; validation stays server-side.'
		: 'Free-form JSON validated server-side; no parameter schema was reported for this tool.'}
>
	{#if toolSchema && Object.keys(toolSchema.properties).length > 0}
		<Segmented
			items={[
				{ id: 'form', label: 'Form' },
				{ id: 'json', label: 'JSON' },
			]}
			value={toolTab}
			size="sm"
			onchange={(id) => switchToolTab(id as 'form' | 'json')}
		/>
		{#if toolTab === 'form'}
			<div class="mt-2 space-y-2">
				{#each Object.entries(toolSchema.properties) as [key, prop] (key)}
					{@const required = toolSchema.required.includes(key)}
					<label class="block">
						<span class="mb-1 block text-caption text-muted-foreground">
							{key}{#if required}<span class="text-destructive"> *</span>{/if}
							{#if prop.description}
								<span class="ml-1 text-micro">· {prop.description}</span>
							{/if}
						</span>
						{#if prop.enum && prop.enum.length > 0}
							<Select
								value={toolForm[key] ?? ''}
								options={prop.enum.map((option) => ({
									value: String(option),
									label: String(option),
								}))}
								size="sm"
								placeholder={key}
								class="w-full"
								onchange={(value) => {
									toolForm = { ...toolForm, [key]: value };
								}}
							/>
						{:else if prop.type === 'boolean'}
							<Select
								value={toolForm[key] ?? ''}
								options={[
									{ value: 'true', label: 'true' },
									{ value: 'false', label: 'false' },
								]}
								size="sm"
								placeholder={key}
								class="w-40"
								onchange={(value) => {
									toolForm = { ...toolForm, [key]: value };
								}}
							/>
						{:else if prop.type === 'object' || prop.type === 'array'}
							<Textarea
								bind:value={toolForm[key]}
								placeholder={prop.description ?? key}
								rows={3}
								class="font-mono text-small"
							/>
						{:else}
							<Input
								bind:value={toolForm[key]}
								placeholder={prop.description ?? key}
								size="sm"
								class="w-full"
							/>
						{/if}
					</label>
				{/each}
				<p class="text-micro text-muted-foreground">
					Form edits typed fields; object and array values use JSON fragments in
					the multiline box. Remaining parameters stay in JSON mode.
				</p>
			</div>
		{:else}
			<div class="mt-2">
				<JsonEditor
					bind:this={toolEditor}
					bind:value={toolParams}
					errorLine={toolErrorLine}
					placeholder={'{\n  "input": "value"\n}'}
					label="Tool parameters"
					minHeight="8rem"
				/>
			</div>
		{/if}
	{:else}
		<JsonEditor
			bind:this={toolEditor}
			bind:value={toolParams}
			errorLine={toolErrorLine}
			placeholder={'{\n  "input": "value"\n}'}
			label="Tool parameters"
			minHeight="8rem"
		/>
	{/if}
	{#if toolSyntaxError}
		<div
			class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
		>
			<p class="text-caption text-destructive">
				JSON syntax{#if toolErrorLine}
					(line {toolErrorLine}){/if}: {toolSyntaxError}
			</p>
			{#if toolErrorLine}
				<Button
					variant="ghost"
					size="sm"
					onclick={() => toolEditor?.scrollToLine(toolErrorLine ?? 1)}
				>
					Go to line
				</Button>
			{/if}
		</div>
	{/if}
	{#if toolIssues.length > 0}
		<div class="mt-2">
			<IssueList issues={toolTemplateIssues} />
		</div>
	{:else if toolShowValid}
		<p class="mt-2 text-caption text-success">Parameters are valid.</p>
	{:else if toolValidationStale}
		<p class="mt-2 text-caption text-muted-foreground">
			Parameters changed after validation; validate again.
		</p>
	{/if}
	{#if toolRun}
		<div class="mt-2 rounded-md border border-border bg-muted/40 p-2">
			<p class="text-caption">
				{toolRun.success ? 'Succeeded' : `Failed: ${toolRun.error}`}
			</p>
			{#if toolRun.output}
				<pre
					class="mt-1 max-h-48 overflow-auto font-mono text-micro">{toolRun.output}</pre>
			{/if}
			<p class="mt-1 text-micro text-muted-foreground">
				{toolRun.durationMs}ms · {toolRun.retries} retries
			</p>
		</div>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={toolBusy}
				onclick={() => void runToolValidation()}
			>
				Validate
			</Button>
			<Button
				size="sm"
				disabled={toolBusy}
				onclick={() => void runToolExecute()}
			>
				{toolBusy ? 'Running…' : 'Run'}
			</Button>
		</div>
	{/snippet}
</Dialog>
