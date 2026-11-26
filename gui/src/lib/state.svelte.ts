import { NORMALLY, part, summarize, type ButtonId, type PartId } from './actions';
import { api, onDaemonChange } from './api';
import type { BindingView, ConfigView, Hosts, Snapshot, ThumbView } from './types';
import { appVersion, checkForUpdate, relaunch, type UpdateHandle } from './updates';

const THUMB_MODES: Record<string, string> = {
	hscroll: 'Scrolls sideways at its own speed',
	volume: 'Changes the volume',
	zoom: 'Zooms in and out',
	tabs: 'Switches tabs',
	none: 'Does nothing'
};

const CHARGING = new Set(['recharging', 'almost-full', 'slow-recharge']);

const UPDATE_EVERY = 6 * 60 * 60 * 1000;
/** Where the choice to look for updates is kept, in this window's own storage. */
const CHECK_UPDATES = 'familiar.check-updates';

/** Everything the window shows, kept in step with the daemon. */
class AppState {
	snapshot = $state.raw<Snapshot | null>(null);
	config = $state.raw<ConfigView | null>(null);
	waveforms = $state.raw<string[]>([]);
	/** False until the first answer, so the window does not flash an empty state. */
	loaded = $state(false);
	/** The part whose settings are open, or null for the overview. */
	selected = $state<PartId | null>(null);
	/** The part under the pointer, on the drawing or in a list. */
	hovered = $state<PartId | null>(null);
	view = $state<'top' | 'under'>('top');
	about = $state(false);
	hosts = $state.raw<Hosts | null>(null);
	/** The last change that went wrong, shown until the next one works. */
	problem = $state<string | null>(null);
	/** Why the settings file could not be read, shown until it can. */
	configProblem = $state<string | null>(null);
	/** Bumped when a control's behaviour is saved, so the plate can acknowledge it. */
	pulse = $state<{ control: string; n: number }>({ control: '', n: 0 });

	/** The version of this window. */
	version = $state('');
	/** Whether this copy can replace itself, which the .deb and .rpm packages can. */
	selfUpdates = $state(false);
	/** Whether to look for a newer release at launch and every few hours. */
	checkUpdates = $state(true);
	/** A newer release, until it is installed. */
	update = $state.raw<UpdateHandle | null>(null);
	/** Download progress from 0 to 1 while an update installs. */
	updateProgress = $state<number | null>(null);
	/** What a check asked for by hand found, or why an update failed. */
	updateNote = $state<{ text: string; bad: boolean } | null>(null);

	device = $derived(this.snapshot?.devices.find((d) => d.online) ?? this.snapshot?.devices[0] ?? null);
	ready = $derived(this.device?.online ?? false);
	charging = $derived(CHARGING.has(this.device?.batteryStatus ?? ''));

	#timer: ReturnType<typeof setTimeout> | undefined;
	#updateTimer: ReturnType<typeof setInterval> | undefined;
	/** Lets a refresh that started before a change drop its stale answer. */
	#seq = 0;
	/** Changes to the settings file that are still being saved. */
	#writes = 0;

	async refresh() {
		const n = ++this.#seq;
		const [snapshot, config] = await Promise.allSettled([api.snapshot(), api.config()]);
		if (n !== this.#seq) return;
		const reachable = this.ready ? this.device?.path : null;
		if (snapshot.status === 'fulfilled') this.snapshot = snapshot.value;
		else this.problem = String(snapshot.reason);
		// The computers may have changed while the mouse was away.
		if (this.ready && this.device?.path !== reachable) this.loadHosts();
		if (config.status === 'fulfilled') {
			// A change still being saved is newer than the file was when it was read.
			if (this.#writes === 0) this.config = config.value;
			this.configProblem = null;
		} else {
			this.configProblem = String(config.reason);
		}
		this.loaded = true;
	}

	/** Coalesces bursts of change signals into one refresh. */
	soon() {
		clearTimeout(this.#timer);
		this.#timer = setTimeout(() => this.refresh(), 120);
	}

	async start() {
		api.waveforms().then((w) => (this.waveforms = w));
		this.startUpdates();
		await this.refresh();
		await onDaemonChange(() => this.soon());
	}

	async startUpdates() {
		try {
			this.checkUpdates = localStorage.getItem(CHECK_UPDATES) !== 'off';
		} catch {
			// Storage can be unavailable; the default stands.
		}
		try {
			this.version = await appVersion();
			this.selfUpdates = await api.selfUpdates();
		} catch {
			this.selfUpdates = false;
		}
		this.scheduleUpdateChecks();
	}

	setCheckUpdates(on: boolean) {
		this.checkUpdates = on;
		try {
			localStorage.setItem(CHECK_UPDATES, on ? 'on' : 'off');
		} catch {
			// The choice then lasts until the window closes.
		}
		this.scheduleUpdateChecks();
	}

	/** Checks for a newer release now and every few hours, while the choice is on. */
	scheduleUpdateChecks() {
		clearInterval(this.#updateTimer);
		this.#updateTimer = undefined;
		if (!this.selfUpdates || !this.checkUpdates) return;
		void this.checkForUpdate();
		this.#updateTimer = setInterval(() => void this.checkForUpdate(), UPDATE_EVERY);
	}

	/** Looks for a newer release. Only a check asked for by hand reports what it found. */
	async checkForUpdate(byHand = false) {
		if (!this.selfUpdates || this.updateProgress !== null) return;
		try {
			this.update = await checkForUpdate();
			if (byHand) this.updateNote = this.update ? null : { text: 'Familiar is up to date.', bad: false };
		} catch (e) {
			if (byHand) this.updateNote = { text: `Could not check for updates: ${e}`, bad: true };
		}
	}

	/** Downloads and installs the release found, restarts the daemon, then this window. */
	async installUpdate() {
		const update = this.update;
		if (!update || this.updateProgress !== null) return;
		this.updateProgress = 0;
		this.updateNote = null;
		try {
			await update.install((fraction) => (this.updateProgress = fraction));
			// The new daemon takes over at the next login if this does not work.
			await api.restartDaemon().catch(() => {});
			await relaunch();
		} catch (e) {
			this.updateNote = { text: `The update could not be installed: ${e}`, bad: true };
		} finally {
			this.updateProgress = null;
		}
	}

	async loadHosts() {
		const path = this.ready ? this.device?.path : null;
		if (!path) return;
		try {
			this.hosts = await api.hosts(path);
		} catch {
			this.hosts = null;
		}
	}

	/** Opens a part's settings, turning the drawing over if it is on the other side. */
	open(id: PartId | null) {
		this.about = false;
		this.selected = id;
		if (id) this.view = part(id).side;
		if (id === 'easy-switch') this.loadHosts();
	}

	/** What a part does now, in a few words. */
	summary(id: PartId): string {
		const s = this.device?.settings ?? {};
		const c = this.config;
		switch (id) {
			case 'wheel': {
				const turn = s.wheel === 'free' ? 'Free spin' : 'Ratchet';
				const shift = s.smartshift && s.smartshift !== 'off' ? ', lets go when flicked' : '';
				return `${turn}${shift}${c?.smooth ? ', smooth' : ''}`;
			}
			case 'thumb-wheel':
				return c?.thumb ? (THUMB_MODES[c.thumb.mode] ?? 'Scrolls sideways') : 'Scrolls sideways';
			case 'sensor':
				return s.dpi ? `${s.dpi} DPI` : 'Pointer speed';
			case 'easy-switch': {
				const h = this.hosts?.hosts.find((x) => x.channel === this.hosts?.current);
				return h ? `Computer ${h.channel}${h.name ? `, ${h.name}` : ''}` : 'Three computers';
			}
			case 'power': {
				const d = this.device;
				if (!d?.online) return 'Asleep';
				if (this.charging) return `Charging, ${d.batteryPercent}%`;
				return d.batteryPercent ? `${d.batteryPercent}%` : 'Level unknown';
			}
			case 'haptic':
				return summarize(c?.buttons.haptic, 'haptic');
			default: {
				const b = id as ButtonId;
				return c ? summarize(c.buttons[b], b) : NORMALLY[b];
			}
		}
	}

	/** Runs a change, reporting failure in the window rather than throwing. */
	async attempt(work: () => Promise<unknown>, control?: string): Promise<boolean> {
		try {
			await work();
			this.problem = null;
			if (control) this.pulse = { control, n: this.pulse.n + 1 };
			return true;
		} catch (e) {
			this.problem = String(e);
			return false;
		} finally {
			this.soon();
		}
	}

	/**
	 * Saves a change to the settings file. The window shows it at once, so the
	 * next change builds on it, and the refresh that follows puts things right
	 * if saving failed.
	 */
	async #write(change: (c: ConfigView) => ConfigView, work: () => Promise<unknown>, control: string) {
		if (this.config) this.config = change(this.config);
		this.#seq++;
		this.#writes++;
		try {
			return await this.attempt(work, control);
		} finally {
			this.#writes--;
		}
	}

	/** Sets what a button does, or with null gives it back to the mouse. */
	setButton(button: ButtonId, binding: BindingView | null, control: string) {
		const change = (c: ConfigView) => {
			const buttons = { ...c.buttons };
			if (binding) buttons[button] = binding;
			else delete buttons[button];
			return { ...c, buttons };
		};
		return this.#write(change, () => api.setButton(button, binding), control);
	}

	setScroll(smooth: boolean, speed: number) {
		return this.#write((c) => ({ ...c, smooth, speed }), () => api.setScroll(smooth, speed), 'wheel');
	}

	setThumb(thumb: ThumbView | null) {
		return this.#write((c) => ({ ...c, thumb }), () => api.setThumb(thumb), 'thumb-wheel');
	}

	setting(name: string, value: string, control?: string) {
		const path = this.device?.path;
		if (!path) return Promise.resolve(false);
		return this.attempt(async () => {
			const settings = await api.setSetting(path, name, value);
			this.#seq++;
			if (this.snapshot) {
				const devices = this.snapshot.devices.map((d) => (d.path === path ? { ...d, settings } : d));
				this.snapshot = { ...this.snapshot, devices };
			}
		}, control);
	}
}

export const app = new AppState();
