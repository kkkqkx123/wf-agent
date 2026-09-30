/**
 * Public surface of the shared UI kit.
 *
 * Components are also reachable one-by-one through the `components/*` and
 * `icons/*` subpath exports; this barrel exists for callers that prefer a
 * single import site.
 */

export { default as Badge } from './components/Badge.svelte';
export { default as Button } from './components/Button.svelte';
export { default as Card } from './components/Card.svelte';
export { default as ContextMenu } from './components/ContextMenu.svelte';
export { default as CursorPager } from './components/CursorPager.svelte';
export { default as DataTable } from './components/DataTable.svelte';
export { default as Dialog } from './components/Dialog.svelte';
export { default as DiffView } from './components/DiffView.svelte';
export { default as DropdownMenu } from './components/DropdownMenu.svelte';
export { default as EmptyState } from './components/EmptyState.svelte';
export { default as ErrorState } from './components/ErrorState.svelte';
export { default as FilterBar } from './components/FilterBar.svelte';
export { default as IconButton } from './components/IconButton.svelte';
export { default as Input } from './components/Input.svelte';
export { default as JsonEditor } from './components/JsonEditor.svelte';
export { default as JsonViewer } from './components/JsonViewer.svelte';
export { default as Progress } from './components/Progress.svelte';
export { default as Segmented } from './components/Segmented.svelte';
export { default as Select } from './components/Select.svelte';
export { default as Separator } from './components/Separator.svelte';
export { default as Sheet } from './components/Sheet.svelte';
export { default as Skeleton } from './components/Skeleton.svelte';
export { default as StatusBadge } from './components/StatusBadge.svelte';
export { default as Switch } from './components/Switch.svelte';
export { default as Textarea } from './components/Textarea.svelte';
export { default as Tooltip } from './components/Tooltip.svelte';
export { default as UnsavedChangesDialog } from './components/UnsavedChangesDialog.svelte';
export { default as Icon } from './icons/Icon.svelte';

export * from './components/table';
export * from './components/variants';
export * from './cn';
export * from './status';
export * from './format';
export * from './virtualization';
export * from './link';
