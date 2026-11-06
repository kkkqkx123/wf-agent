/**
 * Editor-side model registry for the IDE.
 *
 * Mirrors the reference separation of models and editors: a model owns the
 * file content and is identified by a `file:///` URI, while the editor is a
 * transient view onto whichever model is active. Models and per-file view
 * states live here so the tab store (which knows tab lifetimes) and the
 * editor component (which knows the monaco runtime) share one source of
 * truth without importing each other.
 *
 * There is intentionally no size-based eviction: the tab store caps open
 * tabs, which bounds this registry, and a model is released only when its
 * tab closes, so an open file can never lose its model under pressure.
 */

type Monaco = typeof import('monaco-editor');
type MonacoEditorInstance = ReturnType<Monaco['editor']['create']>;
type MonacoModel = ReturnType<Monaco['editor']['createModel']>;

export type MonacoViewState = NonNullable<ReturnType<MonacoEditorInstance['saveViewState']>>;

const models = new Map<string, MonacoModel>();
const viewStates = new Map<string, MonacoViewState>();

export function modelKeyFor(path: string): string {
	return `ide:${path}`;
}

function uriForPath(monaco: Monaco, path: string): ReturnType<Monaco['Uri']['parse']> {
	const encoded = path
		.split('/')
		.map((segment) => encodeURIComponent(segment))
		.join('/');
	return monaco.Uri.parse(`file:///${encoded}`);
}

/**
 * Return the cached model for `path`, creating it with a `file:///` URI on
 * first use. A cached model whose content drifted (external reload) is
 * synced to the store value; the caller restores the view state after.
 */
export function getOrCreateFileModel(
	monaco: Monaco,
	path: string,
	value: string,
	language: string,
): MonacoModel {
	const key = modelKeyFor(path);
	const hit = models.get(key);
	if (hit) {
		monaco.editor.setModelLanguage(hit, language);
		if (hit.getValue() !== value) hit.setValue(value);
		return hit;
	}
	const model = monaco.editor.createModel(value, language, uriForPath(monaco, path));
	models.set(key, model);
	return model;
}

/** Dispose the model and drop its view state once no tab references `path`. */
export function releaseFileModel(path: string): void {
	const key = modelKeyFor(path);
	viewStates.delete(key);
	const model = models.get(key);
	if (!model) return;
	models.delete(key);
	try {
		model.dispose();
	} catch {
		// Disposal is best-effort; the cache entry is already dropped.
	}
}

/**
 * Move a model entry across a rename. A shadowed target model is disposed
 * first so the moved model owns the target key, matching the tab store
 * which keeps the moved tab and drops the shadowed one.
 */
export function retargetFileModel(from: string, to: string): void {
	const fromKey = modelKeyFor(from);
	const toKey = modelKeyFor(to);
	if (fromKey === toKey) return;
	viewStates.delete(toKey);
	const remembered = viewStates.get(fromKey);
	viewStates.delete(fromKey);
	if (remembered) viewStates.set(toKey, remembered);
	const shadowed = models.get(toKey);
	const moved = models.get(fromKey);
	models.delete(fromKey);
	if (shadowed && shadowed !== moved) {
		try {
			shadowed.dispose();
		} catch {
			// Disposal is best-effort; the cache entry is already dropped.
		}
	}
	if (moved) models.set(toKey, moved);
}

/** Remember the view state (cursor, scroll) of `path` when leaving it. */
export function stashViewState(path: string, state: MonacoViewState | null): void {
	const key = modelKeyFor(path);
	if (!state) {
		viewStates.delete(key);
		return;
	}
	viewStates.set(key, state);
}

/** Recall the stored view state of `path` without consuming it. */
export function recallViewState(path: string): MonacoViewState | undefined {
	return viewStates.get(modelKeyFor(path));
}

let workersReady = false;
let workersInflight: Promise<void> | null = null;

/**
 * Register Monaco web workers. Resolves in every case: worker failure only
 * costs local syntax and suggestion features, and a failed attempt stays
 * retryable because readiness is recorded on success only.
 */
export function ensureIdeWorkers(): Promise<void> {
	if (workersReady) return Promise.resolve();
	if (workersInflight) return workersInflight;
	const flag =
		typeof import.meta !== 'undefined'
			? (import.meta.env?.['VITE_MONACO_NO_WORKERS'] as string | undefined)
			: undefined;
	if (flag === '1') return Promise.resolve();
	workersInflight = (async () => {
		try {
			const [
				{ default: EditorWorker },
				{ default: TsWorker },
				{ default: CssWorker },
				{ default: HtmlWorker },
				{ default: JsonWorker },
			] = await Promise.all([
				import('monaco-editor/esm/vs/editor/editor.worker?worker'),
				import('monaco-editor/esm/vs/language/typescript/ts.worker?worker'),
				import('monaco-editor/esm/vs/language/css/css.worker?worker'),
				import('monaco-editor/esm/vs/language/html/html.worker?worker'),
				import('monaco-editor/esm/vs/language/json/json.worker?worker'),
			]);
			(
				globalThis as unknown as {
					MonacoEnvironment?: { getWorker(workerId: unknown, label: string): Worker };
				}
			).MonacoEnvironment = {
				getWorker(_workerId: unknown, label: string): Worker {
					if (label === 'json') return new JsonWorker();
					if (label === 'css' || label === 'scss' || label === 'less') return new CssWorker();
					if (label === 'html' || label === 'handlebars' || label === 'razor')
						return new HtmlWorker();
					if (label === 'typescript' || label === 'javascript') return new TsWorker();
					return new EditorWorker();
				},
			};
			workersReady = true;
		} catch {
			// Keep the no-worker path; the editor still opens with degraded
			// language features.
		} finally {
			workersInflight = null;
		}
	})();
	return workersInflight;
}

let preloadStarted = false;

/**
 * Warm the editor runtime during idle time after first paint so the first
 * file open never pays the module transform. Safe to call repeatedly; a
 * failed attempt resets and retries on the next call.
 */
export function preloadIdeEditor(): void {
	if (preloadStarted || typeof window === 'undefined') return;
	preloadStarted = true;
	void ensureIdeWorkers()
		.then(() => import('monaco-editor'))
		.catch(() => {
			preloadStarted = false;
		});
}
