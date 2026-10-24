import type cytoscape from 'cytoscape';

import { browser } from '$app/env';

/**
 * Canvas stylesheet. Data mappers (`data(field)`) are core cytoscape
 * behavior but postdate the shipped stylesheet types, hence the cast.
 * Static fallbacks use light-theme token values so no themed hex appears.
 */
export const CANVAS_STYLESHEET = [
	{
		selector: 'node',
		style: {
			shape: 'data(shape)',
			width: 120,
			height: 40,
			'background-color': 'hsl(0 0% 100%)',
			'background-opacity': 0.9,
			'border-width': 1.5,
			'border-color': 'data(color)',
			label: 'data(label)',
			color: 'hsl(220 20% 12%)',
			'font-size': 12,
			'text-valign': 'center',
			'text-halign': 'center',
			'text-wrap': 'ellipsis',
			'text-max-width': 104,
		},
	},
	{
		selector: 'node.selected',
		style: {
			'border-width': 3,
			'border-color': 'hsl(220 20% 40%)',
		},
	},
	{
		selector: 'node.problem',
		style: {
			'border-width': 2.5,
			'border-color': 'hsl(0 72% 48%)',
			'border-style': 'dashed',
		},
	},
	{
		selector: 'node.failed',
		style: {
			'border-width': 3,
			'border-color': 'hsl(0 72% 48%)',
		},
	},
	{
		selector: 'node.running',
		style: {
			'border-width': 3,
			'border-color': 'hsl(252 56% 54%)',
		},
	},
	{
		selector: 'node.critical',
		style: {
			'border-width': 2.5,
			'border-color': 'hsl(32 88% 44%)',
		},
	},
	{
		selector: 'node.decision',
		style: {
			'border-width': 2.5,
			'border-color': 'hsl(280 52% 56%)',
			'border-style': 'dashed',
		},
	},
	{
		selector: 'node.heat-1',
		style: {
			'border-width': 2,
			'border-color': 'hsl(32 88% 44%)',
		},
	},
	{
		selector: 'node.heat-2',
		style: {
			'border-width': 2.5,
			'border-color': 'hsl(24 95% 53%)',
		},
	},
	{
		selector: 'node.heat-3',
		style: {
			'border-width': 3,
			'border-color': 'hsl(0 72% 48%)',
		},
	},
	{
		selector: 'node.highlighted',
		style: {
			'border-width': 3,
			'border-color': 'hsl(32 88% 44%)',
		},
	},
	{
		selector: 'node.dimmed',
		style: { opacity: 0.3 },
	},
	{
		selector: 'edge',
		style: {
			width: 1.5,
			'line-color': 'data(color)',
			'line-style': 'data(lineStyle)',
			'target-arrow-shape': 'triangle',
			'target-arrow-color': 'data(color)',
			'curve-style': 'bezier',
			label: 'data(label)',
			color: 'hsl(215 12% 46%)',
			'font-size': 11,
			'edge-text-rotation': 'autorotate',
			'text-background-color': 'hsl(0 0% 100%)',
			'text-background-opacity': 0.7,
			'text-background-padding': 2,
		},
	},
	{
		selector: 'edge.fail-branch',
		style: {
			'line-style': 'dashed',
			width: 2,
		},
	},
	{
		selector: 'edge.gradient',
		style: {
			'line-fill': 'linear-gradient',
			'line-gradient-stop-colors': 'data(gradient)',
			'line-gradient-stop-positions': '0% 100%',
			'target-arrow-color': 'data(targetColor)',
		},
	},
	{
		selector: 'edge.waiting',
		style: { opacity: 0.55 },
	},
	{
		selector: 'node.group-title',
		style: {
			width: 160,
			'border-width': 2,
			'border-style': 'dashed',
			'border-color': 'hsl(215 12% 46%)',
		},
	},
	{
		selector: 'node.group-title.failed',
		style: {
			'border-width': 3,
			'border-style': 'solid',
			'border-color': 'hsl(0 72% 48%)',
		},
	},
	{
		selector: 'node.group-title.running',
		style: {
			'border-width': 3,
			'border-style': 'solid',
			'border-color': 'hsl(252 56% 54%)',
		},
	},
	{
		selector: 'node.group-title.problem',
		style: {
			'border-width': 2.5,
			'border-style': 'dashed',
			'border-color': 'hsl(0 72% 48%)',
		},
	},
	{
		selector: 'node.group-title.critical',
		style: {
			'border-width': 2.5,
			'border-style': 'dashed',
			'border-color': 'hsl(32 88% 44%)',
		},
	},
	{
		selector: 'node.group-box',
		style: {
			'background-opacity': 0.15,
			'background-color': 'hsl(214 20% 96%)',
			'border-width': 1,
			'border-style': 'dashed',
			'border-color': 'hsl(214 16% 91%)',
			label: 'data(label)',
			color: 'hsl(215 12% 46%)',
			'font-size': 11,
			'text-valign': 'top',
			'text-halign': 'center',
			padding: '14px',
		},
	},
	{
		selector: 'node.connect-ok',
		style: {
			'border-width': 3,
			'border-color': 'hsl(152 55% 38%)',
		},
	},
	{
		selector: 'node.connect-bad',
		style: { opacity: 0.45 },
	},
	{
		selector: 'edge.dimmed',
		style: { opacity: 0.25 },
	},
] as unknown as cytoscape.StylesheetJson;

/**
 * Theme bridge for the canvas. Cytoscape parses its own color values and
 * cannot consume CSS variables, so the active stylesheet is derived by
 * reading the computed semantic tokens and emitting opaque color strings.
 * Outside the browser the static fallback above is used as-is.
 */
function readToken(name: string): string | null {
	if (!browser || typeof document === 'undefined') return null;
	const raw = getComputedStyle(document.documentElement)
		.getPropertyValue(name)
		.trim();
	return raw ? `hsl(${raw})` : null;
}

function readFontToken(name: string, fallback: number): number {
	if (!browser || typeof document === 'undefined') return fallback;
	const raw = getComputedStyle(document.documentElement)
		.getPropertyValue(name)
		.trim();
	if (!raw) return fallback;
	// Custom properties are returned as specified, so rem/calc values need
	// resolving through a probe element to obtain concrete pixels.
	const direct = Number.parseFloat(raw);
	if (/px$/i.test(raw) && Number.isFinite(direct) && direct > 0) {
		return direct;
	}
	const probe = document.createElement('span');
	probe.style.fontSize = `var(${name})`;
	probe.style.position = 'absolute';
	probe.style.visibility = 'hidden';
	document.body.appendChild(probe);
	const computed = getComputedStyle(probe).fontSize;
	document.body.removeChild(probe);
	const resolved = Number.parseFloat(computed);
	return Number.isFinite(resolved) && resolved > 0 ? resolved : fallback;
}

/** Node and edge font sizes follow the canvas type tokens. */
export function canvasFontSizes(): { node: number; edge: number } {
	return {
		node: readFontToken('--canvas-node-font-size', readFontToken('--text-small', 12)),
		edge: readFontToken('--canvas-edge-font-size', readFontToken('--text-micro', 11)),
	};
}

/** Compensation factor for the current zoom; rings stay legible zoomed out. */
export function canvasZoomCompensation(zoom: number): number {
	if (!Number.isFinite(zoom) || zoom <= 0) return 1;
	return Math.min(2, Math.max(0.5, 1 / zoom));
}

/** Whether the canvas should skip continuous motion for this user. */
export function prefersReducedMotion(): boolean {
	if (!browser || typeof window === 'undefined') return false;
	return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Running and waiting outline durations; reduced motion collapses them. */
export function canvasRunDurations(): { running: number; waiting: number } {
	if (prefersReducedMotion()) return { running: 0, waiting: 0 };
	return { running: 1500, waiting: 4500 };
}

/** Base ring widths before zoom compensation. */
const ZOOM_RING_WIDTH: Record<string, number> = {
	'node.selected': 3,
	'node.failed': 3,
	'node.running': 3,
	'node.heat-3': 3,
	'node.highlighted': 3,
};

/** Ring width for a selector at the given zoom. */
export function zoomRingWidth(selector: string, zoom: number): number {
	const base = ZOOM_RING_WIDTH[selector] ?? 2.5;
	const compensated = base * canvasZoomCompensation(zoom);
	return Math.round(compensated * 2) / 2;
}

/** Active canvas palette: semantic tokens with static fallbacks. */
export function canvasPalette(): Record<string, string> {
	const token = (name: string, fallback: string): string =>
		readToken(name) ?? fallback;
	return {
		nodeBg: token('--card', 'hsl(0 0% 100%)'),
		nodeFg: token('--card-foreground', 'hsl(220 20% 12%)'),
		edgeLabelFg: token('--muted-foreground', 'hsl(215 12% 46%)'),
		edgeLabelBg: token('--card', 'hsl(0 0% 100%)'),
		groupBorder: token('--muted-foreground', 'hsl(215 12% 46%)'),
		groupBoxBg: token('--muted', 'hsl(214 20% 96%)'),
		groupBoxBorder: token('--border', 'hsl(214 16% 91%)'),
		groupBoxFg: token('--muted-foreground', 'hsl(215 12% 46%)'),
		selection: token('--ring', 'hsl(220 20% 40%)'),
		problem: token('--destructive', 'hsl(0 72% 48%)'),
		failed: token('--destructive', 'hsl(0 72% 48%)'),
		running: token('--running', 'hsl(252 56% 54%)'),
		critical: token('--warning', 'hsl(32 88% 44%)'),
		highlight: token('--warning', 'hsl(32 88% 44%)'),
		decision: token('--chart-4', 'hsl(280 52% 56%)'),
		heat1: token('--warning', 'hsl(32 88% 44%)'),
		heat2: token('--brand', 'hsl(24 95% 53%)'),
		heat3: token('--destructive', 'hsl(0 72% 48%)'),
		connectOk: token('--success', 'hsl(152 55% 38%)'),
	};
}

/**
 * Stylesheet for the live canvas instance. Structural values stay shared
 * with the static fallback; only colors and type sizes are resolved
 * against the active theme, and ring widths scale with the zoom factor.
 */
export function resolveCanvasStylesheet(zoom = 1): cytoscape.StylesheetJson {
	if (!browser || typeof document === 'undefined') return CANVAS_STYLESHEET;
	const palette = canvasPalette();
	const fonts = canvasFontSizes();
	const base = CANVAS_STYLESHEET as unknown as Array<{
		selector: string;
		style: Record<string, unknown>;
	}>;
	const overrides: Record<string, Record<string, unknown>> = {
		node: {
			'background-color': palette.nodeBg,
			color: palette.nodeFg,
			'font-size': fonts.node,
		},
		'node.selected': {
			'border-color': palette.selection,
			'border-width': zoomRingWidth('node.selected', zoom),
		},
		'node.problem': { 'border-color': palette.problem },
		'node.failed': {
			'border-color': palette.failed,
			'border-width': zoomRingWidth('node.failed', zoom),
		},
		'node.running': {
			'border-color': palette.running,
			'border-width': zoomRingWidth('node.running', zoom),
		},
		'node.critical': { 'border-color': palette.critical },
		'node.decision': { 'border-color': palette.decision },
		'node.heat-1': { 'border-color': palette.heat1 },
		'node.heat-2': { 'border-color': palette.heat2 },
		'node.heat-3': {
			'border-color': palette.heat3,
			'border-width': zoomRingWidth('node.heat-3', zoom),
		},
		'node.highlighted': {
			'border-color': palette.highlight,
			'border-width': zoomRingWidth('node.highlighted', zoom),
		},
		edge: {
			color: palette.edgeLabelFg,
			'font-size': fonts.edge,
			'text-background-color': palette.edgeLabelBg,
		},
		'node.group-title': { 'border-color': palette.groupBorder },
		'node.group-title.failed': { 'border-color': palette.failed },
		'node.group-title.running': { 'border-color': palette.running },
		'node.group-title.problem': { 'border-color': palette.problem },
		'node.group-title.critical': { 'border-color': palette.critical },
		'node.group-box': {
			'background-color': palette.groupBoxBg,
			'border-color': palette.groupBoxBorder,
			color: palette.groupBoxFg,
			'font-size': fonts.edge,
		},
		'node.connect-ok': { 'border-color': palette.connectOk },
	};
	return base.map((entry) => ({
		selector: entry.selector,
		style: { ...entry.style, ...(overrides[entry.selector] ?? {}) },
	})) as unknown as cytoscape.StylesheetJson;
}
