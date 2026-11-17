<script lang="ts">
	import { describe, eventModifiers, keyName, modifierName } from './actions';

	interface Props {
		/** The keys, such as `ctrl+v` or `KEY_LEFTALT+KEY_TAB`. */
		value: string;
		onchange: (keys: string) => void;
		/** Start listening as soon as it appears. */
		listen?: boolean;
		describedby?: string;
	}

	let { value, onchange, listen = false, describedby }: Props = $props();

	const ORDER = ['ctrl', 'shift', 'alt', 'altgr', 'super'] as const;
	const LABELS: Record<string, string> = { ctrl: 'Ctrl', shift: 'Shift', alt: 'Alt', altgr: 'AltGr', super: 'Super' };
	const MOD_OF: Record<string, string> = {
		CTRL: 'ctrl',
		LEFTCTRL: 'ctrl',
		RIGHTCTRL: 'ctrl',
		SHIFT: 'shift',
		LEFTSHIFT: 'shift',
		RIGHTSHIFT: 'shift',
		ALT: 'alt',
		LEFTALT: 'alt',
		ALTGR: 'altgr',
		RIGHTALT: 'altgr',
		SUPER: 'super',
		META: 'super',
		LEFTMETA: 'super',
		RIGHTMETA: 'super'
	};

	function parse(v: string) {
		const mods: string[] = [];
		const keys: string[] = [];
		for (const part of v.split('+').filter(Boolean)) {
			const m = MOD_OF[part.toUpperCase().replace(/^KEY_/, '')];
			if (!m) keys.push(part);
			else if (!mods.includes(m)) mods.push(m);
		}
		return { mods, key: keys.join('+') };
	}

	const ordered = (mods: string[]) => ORDER.filter((m) => mods.includes(m));

	// The string alone, so a refresh that leaves it unchanged keeps local edits.
	let current = $derived(value);
	let mods = $derived(parse(current).mods);
	let key = $derived(parse(current).key);
	let recording = $state(false);
	let unsendable = $state(false);
	/** Modifiers held down during this recording. */
	let down: string[] = [];
	/** Set when a recording ends, so releasing the last key does not press the button. */
	let swallow = false;

	function emit() {
		const keys = [...ordered(mods), key].filter(Boolean).join('+');
		if (keys) onchange(keys);
	}

	function toggle(m: string) {
		mods = mods.includes(m) ? mods.filter((x) => x !== m) : [...mods, m];
		if (key) emit();
	}

	function start(button: HTMLButtonElement) {
		recording = true;
		unsendable = false;
		down = [];
		button.focus();
	}

	function stop() {
		recording = false;
		down = [];
	}

	function finish() {
		stop();
		swallow = true;
		emit();
	}

	function onkeydown(e: KeyboardEvent) {
		if (!recording) return;
		e.preventDefault();
		if (e.key === 'Escape' && !down.length) {
			stop();
			return;
		}
		const m = modifierName(e.code);
		if (m) {
			if (!down.includes(m)) down = [...down, m];
			return;
		}
		const k = keyName(e.code);
		unsendable = !k;
		if (!k) return;
		// Modifiers pressed with the key replace the switches; a key on its own keeps them.
		const held = [...new Set([...down, ...eventModifiers(e)])];
		if (held.length) mods = held;
		key = k;
		finish();
	}

	function onkeyup(e: KeyboardEvent) {
		if (swallow) {
			e.preventDefault();
			swallow = false;
			return;
		}
		// Modifiers pressed and released on their own, such as Super, are a shortcut too.
		if (recording && down.length && modifierName(e.code)) {
			mods = down;
			key = '';
			finish();
		}
	}
</script>

<div class="combo">
	<div class="mods" role="group" aria-label="Held with">
		{#each ORDER as m (m)}
			{#if m !== 'altgr' || mods.includes('altgr')}
				<button
					class={['cap', 'small', 'mod', mods.includes(m) && 'held']}
					aria-pressed={mods.includes(m)}
					aria-describedby={describedby}
					onclick={() => toggle(m)}>{LABELS[m]}</button
				>
			{/if}
		{/each}
	</div>
	<span class="plus" aria-hidden="true">+</span>
	<button
		class={['cap', 'small', 'record', recording && 'live']}
		aria-describedby={describedby}
		{@attach (el) => {
			if (listen) start(el);
		}}
		onclick={(e) => !recording && start(e.currentTarget)}
		{onkeydown}
		{onkeyup}
		onblur={stop}
	>
		{#if recording}
			Press a key
		{:else if key}
			{describe(`key:${key}`)}
		{:else if mods.length}
			On its own
		{:else}
			Choose a key
		{/if}
	</button>
	<p class="note" role="status">
		{#if recording && unsendable}
			That key cannot be sent. Try another.
		{:else if recording}
			Press the whole shortcut, or just its last key. Esc stops.
		{:else}
			Shortcuts your desktop keeps for itself, like Alt+Tab, cannot be pressed here: switch on Alt, then press Tab.
		{/if}
	</p>
</div>

<style>
	.combo {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
	}

	.mods {
		display: flex;
		gap: 0.3rem;
	}

	.mod {
		min-width: 2.9rem;
	}

	.held {
		background: var(--ink);
		color: var(--surface);
		border-color: var(--ink);
		box-shadow: var(--press);
	}

	.plus {
		color: var(--muted);
	}

	.record {
		min-width: 6.5rem;
		justify-content: flex-start;
	}

	/* A lit dot while it listens, like the status light. */
	.record.live::before {
		content: '';
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--led);
		box-shadow: 0 0 6px var(--led-glow);
		animation: listen 0.8s ease-in-out infinite alternate;
	}

	@keyframes listen {
		to {
			opacity: 0.3;
		}
	}

	.note {
		flex-basis: 100%;
		font-size: var(--t-xs);
		color: var(--muted);
	}
</style>
