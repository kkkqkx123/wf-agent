export type ToastTone = 'success' | 'danger' | 'warning' | 'info';

export interface ToastAction {
	label: string;
	run: () => void;
}

export interface Toast {
	id: number;
	title: string;
	description?: string;
	tone: ToastTone;
	action?: ToastAction;
	timeout: number;
}

interface ToastInput {
	title: string;
	description?: string;
	tone?: ToastTone;
	action?: ToastAction;
	timeout?: number;
}

const DEFAULT_TIMEOUT = 4500;

class ToastStore {
	items = $state<Toast[]>([]);

	private counter = 0;
	private timers = new Map<number, ReturnType<typeof setTimeout>>();

	push(input: ToastInput): number {
		const id = (this.counter += 1);
		const toast: Toast = {
			id,
			title: input.title,
			description: input.description,
			tone: input.tone ?? 'info',
			action: input.action,
			timeout: input.timeout ?? DEFAULT_TIMEOUT,
		};
		this.items = [...this.items, toast];
		this.scheduleDismiss(id, toast.timeout);
		return id;
	}

	success(title: string, description?: string): number {
		return this.push({ title, description, tone: 'success' });
	}

	error(title: string, description?: string): number {
		return this.push({ title, description, tone: 'danger', timeout: 8000 });
	}

	warning(title: string, description?: string): number {
		return this.push({ title, description, tone: 'warning' });
	}

	info(title: string, description?: string): number {
		return this.push({ title, description, tone: 'info' });
	}

	dismiss(id: number): void {
		const timer = this.timers.get(id);
		if (timer) {
			clearTimeout(timer);
			this.timers.delete(id);
		}
		this.items = this.items.filter((item) => item.id !== id);
	}

	clear(): void {
		this.timers.forEach((timer) => clearTimeout(timer));
		this.timers.clear();
		this.items = [];
	}

	private scheduleDismiss(id: number, timeout: number): void {
		if (timeout <= 0) return;
		this.timers.set(
			id,
			setTimeout(() => this.dismiss(id), timeout),
		);
	}
}

export const toasts = new ToastStore();
