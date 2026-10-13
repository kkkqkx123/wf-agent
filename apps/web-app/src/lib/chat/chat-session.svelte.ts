import { tick } from "svelte";
import { page } from "$app/state";
import { groupToolCalls } from "$lib/utils/tool-output";
import { preferences } from "$lib/stores/preferences.svelte";
import { SvelteSet } from "svelte/reactivity";
import {
	getLoopGraph,
	listLoopMessages,
	type RunLoopMessage,
} from "$lib/services/agent-loops";
import {
	applyExecutionOverlay,
	matchDecisionNodeId,
	nodesForIteration,
	projectExecutionOverlay,
} from "$lib/graph/execution-projection";
import type { DisplayEdge, DisplayNode } from "$lib/graph/display-model";
import { streamLoopRun } from "$lib/services/streaming";
import { listModelProfiles } from "$lib/services/resources";
import type { CommandAction } from "$lib/config/commands";
import type {
	LoopMessage,
	MessageAttachment,
	OutgoingMessage,
	ToolCallEntry,
} from "$lib/types/models";
import { splitAttachments, withAttachments } from "$lib/utils/attachments";
import { createResource } from "$lib/stores/collection.svelte";
import {
	chatStream,
	type LiveToolCall,
	type SubAgentNote,
} from "$lib/stores/stream-run.svelte";
import { NEW_SESSION, sessions } from "$lib/stores/sessions.svelte";
import { toasts } from "$lib/stores/toast.svelte";
import { gotoWithParams } from "$lib/utils/route";
import { isSessionTab, type SessionTab } from "$lib/config/session-tabs";

/**
 * Per-page chat controller: owns selection, the transcript resource, the
 * live-stream tail state, the mini decision map projection and the send
 * pipeline. Route components stay thin and read from this instance.
 */
export class ChatSessionController {
	/** Selection lives in the address, so the sidebar and this page share it. */
	selectedId = $derived(page.url.searchParams.get("id"));

	model = $state("");
	revision = $state(0);

	/** Inspector tab, mirrored into the address by the route. */
	tab = $state<SessionTab>("overview");

	constructor(initialTab: string | undefined) {
		this.tab = isSessionTab(initialTab) ?? "overview";
	}

	transcript = createResource<LoopMessage[]>(async () => {
		if (!this.selectedId) return [];
		return (await listLoopMessages(this.selectedId)).items;
	});

	bubbles = $derived(this.transcript.data ?? []);

	/** Content counter the transcript viewport follows while pinned to the tail. */
	activity = $derived(
		this.bubbles.length +
			chatStream.answer.length +
			chatStream.reasoning.length +
			chatStream.tools.length,
	);

	showLive = $derived(
		chatStream.sessionKey === (this.selectedId ?? NEW_SESSION) &&
			(chatStream.active ||
				chatStream.done ||
				chatStream.cancelled ||
				chatStream.error !== null ||
				chatStream.answer !== ""),
	);

	showTranscript = $derived(this.bubbles.length > 0 || this.showLive);

	session = $derived(
		this.selectedId
			? (sessions.list.items.find((item) => item.id === this.selectedId) ??
				null)
			: null,
	);

	toLiveEntry(tool: LiveToolCall): ToolCallEntry {
		return {
			id: tool.id,
			name: tool.name,
			kind: tool.kind ?? "",
			status: tool.status,
			startedAt: "",
			durationMs: 0,
			input: "",
			output: tool.result,
			error: tool.error,
			displayKind: tool.display?.kind,
			displayPreview: tool.display?.preview,
			displayTruncated: tool.display?.truncated,
		};
	}

	/** Live tool rows: consecutive reads and edits fold into one group row
	 * while shell output and failures keep their own rows. Grouping is a
	 * pure derivation over entry ids, so a collapsed group survives appends.
	 * With previews off the flat pre-grouping list renders instead. */
	liveGroups = $derived(
		groupToolCalls(chatStream.tools.map((tool) => this.toLiveEntry(tool))),
	);
	groupTools = $derived(preferences.toolOutputPreviews);
	openGroups = new SvelteSet<string>();

	toggleGroup(key: string): void {
		if (this.openGroups.has(key)) this.openGroups.delete(key);
		else this.openGroups.add(key);
	}

	// Group expansion is session-local memory: new ids mean new keys, but
	// clear explicitly on session switch so stale keys never accumulate.
	$effect(() => {
		void this.selectedId;
		this.openGroups.clear();
	});

	// Mini decision map: lightweight mirror of the session decision graph,
	// sharing the projection bridge with the inspector graph tab.
	loopGraph = createResource(async () => {
		if (!this.selectedId) return { nodes: [], edges: [] };
		return getLoopGraph(this.selectedId);
	});

	mapOpen = $state(false);
	miniSelected = $state<string | null>(null);
	focusedToolId = $state<string | null>(null);
	miniCanvas = $state<{ zoomTo: (id: string) => void } | null>(null);

	$effect(() => {
		if (!this.selectedId) {
			this.mapOpen = false;
			this.miniSelected = null;
			this.focusedToolId = null;
			return;
		}
		void this.loopGraph.reload();
	});

	miniBaseNodes = $derived<DisplayNode[]>(
		(this.loopGraph.data?.nodes ?? []).map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
			iteration: node.iteration,
		})),
	);
	miniEdges = $derived<DisplayEdge[]>(
		(this.loopGraph.data?.edges ?? []).map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
			kind: edge.kind,
			taken: edge.taken,
		})),
	);

	// Beat-merged snapshot for projection input. Stream frames arrive at
	// SSE cadence; the mini map and inspector only consume this snapshot so
	// high-frequency deltas never re-layout the graph per frame.
	projectedTools = $state<LiveToolCall[]>([]);
	projectedSubAgents = $state<SubAgentNote[]>([]);
	projectedIteration = $state<number | null>(null);

	$effect(() => {
		const id = window.setInterval(() => {
			if (!chatStream.active) return;
			this.projectedTools = [...chatStream.tools];
			this.projectedSubAgents = [...chatStream.subAgents];
			this.projectedIteration = chatStream.iteration;
		}, 200);
		return () => window.clearInterval(id);
	});

	$effect(() => {
		if (chatStream.active) return;
		this.projectedTools = [...chatStream.tools];
		this.projectedSubAgents = [...chatStream.subAgents];
		this.projectedIteration = chatStream.iteration;
	});

	// Converge with the history snapshot once the run settles.
	$effect(() => {
		if (chatStream.done && this.selectedId) void this.loopGraph.reload();
	});

	matchMiniNode(name: string): string | null {
		return matchDecisionNodeId(this.miniBaseNodes, name);
	}

	miniLive = $derived.by(() => {
		const live: Record<string, string> = {};
		let current: string | null = null;
		for (const tool of this.projectedTools) {
			const nodeId = this.matchMiniNode(tool.name);
			if (!nodeId) continue;
			live[nodeId] = tool.status;
			if (tool.status === "running" || tool.status === "pending") {
				current = nodeId;
			}
		}
		for (const agent of this.projectedSubAgents) {
			const nodeId = this.matchMiniNode(agent.name);
			if (!nodeId) continue;
			const status =
				agent.success === null
					? "running"
					: agent.success
						? "completed"
						: "failed";
			live[nodeId] = status;
			if (agent.success === null) current ??= nodeId;
		}
		if (current === null && this.projectedIteration !== null) {
			current =
				nodesForIteration(this.miniBaseNodes, this.projectedIteration)[0] ??
				null;
		}
		return { live, current };
	});

	miniLiveStatuses = $derived(this.miniLive.live);
	miniCurrentNode = $derived(this.miniLive.current);

	miniNodes = $derived.by(() => {
		if (this.miniBaseNodes.length === 0) return this.miniBaseNodes;
		const overlay = projectExecutionOverlay(this.miniBaseNodes, {
			currentNode: this.miniCurrentNode,
			liveStatuses: this.miniLiveStatuses,
		});
		return applyExecutionOverlay(this.miniBaseNodes, overlay);
	});

	focusToolFromNode(nodeId: string): void {
		this.miniSelected = nodeId;
		const node = this.miniBaseNodes.find((entry) => entry.id === nodeId);
		const needle = (node?.label ?? nodeId).toLowerCase();
		const match = [...chatStream.tools]
			.reverse()
			.find(
				(tool) =>
					tool.name.toLowerCase() === needle ||
					tool.name.toLowerCase().includes(nodeId.toLowerCase()),
			);
		if (match) {
			this.focusedToolId = match.id;
			const group = this.liveGroups.find((candidate) =>
				candidate.members.some((member) => member.id === match.id),
			);
			const scroll = (): void => {
				document
					.getElementById(`live-tool-${match.id}`)
					?.scrollIntoView({ block: "nearest", behavior: "smooth" });
			};
			if (
				group &&
				group.kind !== "single" &&
				!this.openGroups.has(group.key)
			) {
				this.openGroups.add(group.key);
				void tick().then(scroll);
				return;
			}
			scroll();
		}
	}

	focusNodeFromTool(toolId: string, toolName: string): void {
		const nodeId = this.matchMiniNode(toolName);
		this.focusedToolId = toolId;
		if (!nodeId) return;
		this.miniSelected = nodeId;
		if (!this.mapOpen) this.mapOpen = true;
		queueMicrotask(() => this.miniCanvas?.zoomTo(nodeId));
	}

	sendText(message: OutgoingMessage): boolean {
		const text = message.text.trim();
		if (!text && message.attachments.length === 0) return false;
		if (chatStream.active) return false;
		if (!this.model.trim()) {
			toasts.error("Choose a model before sending");
			return false;
		}
		void this.runStream(text, message.attachments);
		return true;
	}

	sendPrompt(text: string): boolean {
		return this.sendText({ text, attachments: [] });
	}

	retryLast(): void {
		const bubbles = this.bubbles;
		for (let index = bubbles.length - 1; index >= 0; index -= 1) {
			if (bubbles[index].role === "user") {
				this.sendText(splitAttachments(bubbles[index].content));
				return;
			}
		}
		toasts.error("No user message to retry");
	}

	recordFeedback(kind: "up" | "down"): void {
		if (kind === "up") toasts.success("Thanks for the feedback");
		else toasts.info("Feedback recorded, will improve");
	}

	/** Every slash command maps to one action here; the table owns the vocabulary. */
	COMMANDS: Record<CommandAction, (arg: string) => void> = {
		new: () => sessions.startDraft(),
		retry: () => this.retryLast(),
		continue: (arg) => this.sendPrompt(arg ? `Continue: ${arg}` : "Continue"),
	};

	async runStream(
		content: string,
		files: MessageAttachment[],
	): Promise<void> {
		const draft = !this.selectedId;
		const baseline = new Set(sessions.list.items.map((item) => item.id));
		const startedAt = new Date().toISOString();
		const history: RunLoopMessage[] = this.bubbles.map((message) => ({
			id: message.id,
			role: message.role,
			content: message.content,
			timestamp: Date.parse(message.createdAt) || Date.now(),
		}));
		const signal = chatStream.start(this.selectedId ?? NEW_SESSION);
		await streamLoopRun(
			this.selectedId ?? NEW_SESSION,
			{
				model: this.model.trim(),
				message: withAttachments(content, files),
				conversation: history,
			},
			{
				onDelta: (delta) => chatStream.appendDelta(delta),
				onReasoning: (delta) => chatStream.appendReasoning(delta),
				onIterationStart: (iteration) => {
					chatStream.iteration = iteration;
				},
				onIterationEnd: (iteration) => {
					chatStream.iteration = iteration;
				},
				onToolStart: (toolCallId, toolName, toolKind) =>
					chatStream.toolStart(toolCallId, toolName, toolKind),
				onToolEnd: (tool) =>
					chatStream.toolEnd(
						tool.toolCallId,
						tool.toolName,
						tool.success,
						tool.result,
						tool.toolKind,
						tool.error,
						tool.display,
					),
				onUsage: (usage) => {
					chatStream.usage = usage;
				},
				onSubAgent: (id, name, success) =>
					chatStream.noteSubAgent(id, name, success),
				onCompleted: () => chatStream.complete(),
				onFailed: (message) => chatStream.fail(message),
				onInterrupted: (reason) => chatStream.fail(reason),
				onError: (failure) =>
					chatStream.fail(failure.message, failure.retryAfterMs),
			},
			signal,
		);
		if (!chatStream.error && !chatStream.done) chatStream.complete();
		await sessions.refresh();
		if (draft) {
			const fresh = sessions.sessions.filter(
				(row) => !baseline.has(row.id),
			);
			const adopted =
				fresh.find((row) => row.startedAt >= startedAt) ?? fresh[0];
			if (adopted) {
				sessions.recordTitle(adopted.id, content);
				sessions.open(adopted.id);
			}
		}
		this.revision += 1;
		if (this.selectedId) await this.transcript.reload();
	}

	// The address is the source of truth for which session is open.
	$effect(() => {
		const id = this.selectedId;
		if (!id) {
			this.transcript.data = [];
			return;
		}
		sessions.touch(id);
		void this.transcript.reload();
	});

	/** URL-sync effect body; the route hosts the $effect wrapper. */
	syncAddress(tab: string, panelOpen: boolean): void {
		void gotoWithParams(page.url, {
			id: this.selectedId ?? "",
			tab: tab === "overview" ? "" : tab,
			panel: panelOpen ? "" : "closed",
		});
	}

	refresh(): void {
		this.revision += 1;
		void this.transcript.reload();
	}

	/** Profiles are optional; the composer keeps a manual input. */
	loadDefaultModel(): void {
		void listModelProfiles()
			.then((profiles) => {
				const preferred =
					profiles.find((item) => item.isDefault) ?? profiles[0];
				if (preferred) this.model = preferred.model;
			})
			.catch(() => {});
	}
}
