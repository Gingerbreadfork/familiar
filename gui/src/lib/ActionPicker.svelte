<script lang="ts">
	import KeyCombo from './KeyCombo.svelte';
	import Rocker from './ui/Rocker.svelte';
	import { KINDS, MEDIA, join, split, waveformName, type Kind } from './actions';
	import { api } from './api';
	import { app } from './state.svelte';

	interface Props {
		/** The action string, or null for none chosen yet. */
		value: string | null;
		label: string;
		onchange: (action: string) => void;
		/** Offer holding a shortcut for as long as the button is down, as well as pressing it once. */
		holdable?: boolean;
	}

	let { value, label, onchange, holdable = false }: Props = $props();

	const id = $props.id();
	// The string alone, so a refresh that leaves it unchanged keeps local edits.
	let current = $derived(value);
	// Edits stay local until the action is complete, then the parent saves it.
	let kind = $derived(split(current).kind);
	let arg = $derived(split(current).value);
	let once = $derived(current?.startsWith('press:') ?? false);
	/** True just after choosing a keyboard shortcut, so the keys can be pressed straight away. */
	let fresh = $state(false);

	const DEFAULTS: Partial<Record<Kind, string>> = {
		media: 'volumeup',
		mouse: 'middle',
		'dpi-shift': '400',
		'dpi-cycle': '800,1600,3200',
		dpi: '1600',
		host: '1',
		haptic: 'completed'
	};

	function emit(k: Kind, v: string) {
		const action = join(k, v);
		if (action) onchange(action);
	}

	/** Brings a typed DPI within what the sensor takes, writing it back to the field. */
	function emitDpi(k: Kind, field: HTMLInputElement) {
		const n = Math.round(Number(field.value) / 50) * 50;
		if (!field.value || !Number.isFinite(n)) return;
		field.value = String(Math.min(8000, Math.max(200, n)));
		emit(k, (arg = field.value));
	}

	function emitKeys(keys: string) {
		arg = keys;
		onchange(`${once ? 'press' : 'key'}:${keys}`);
	}

	function pickKind(k: Kind) {
		kind = k;
		arg = DEFAULTS[k] ?? '';
		fresh = k === 'key';
		if (k !== 'key') emit(k, arg);
	}
</script>

<div class="picker">
	<label class="sr" id={`${id}-label`} for={id}>{label}</label>
	<select {id} value={kind} onchange={(e) => pickKind(e.currentTarget.value as Kind)}>
		{#each KINDS as k (k.id)}
			<option value={k.id}>{k.label}</option>
		{/each}
	</select>

	{#if kind === 'key'}
		<KeyCombo
			value={arg}
			listen={fresh}
			describedby={`${id}-label`}
			onchange={(keys) => {
				fresh = false;
				emitKeys(keys);
			}}
		/>
		{#if holdable}
			<div class="hold">
				<Rocker
					label="While the button is down"
					value={once ? 'once' : 'hold'}
					options={[
						{ value: 'hold', label: 'Hold' },
						{ value: 'once', label: 'Press once' }
					]}
					onchange={(v) => {
						once = v === 'once';
						if (arg) emitKeys(arg);
					}}
				/>
				<p class="note">
					{#if once}
						The shortcut is pressed and let go straight away, so Alt+Tab flips to your last window.
					{:else}
						Ctrl, Shift, Alt and Super stay down until you let go, and the key is pressed once. Alt+Tab keeps the
						switcher open while you hold the button.
					{/if}
				</p>
			</div>
		{/if}
	{:else if kind === 'media'}
		<select aria-label="Key" aria-describedby={`${id}-label`} value={arg} onchange={(e) => emit('media', (arg = e.currentTarget.value))}>
			{#each MEDIA as m (m.id)}
				<option value={m.id}>{m.label}</option>
			{/each}
		</select>
	{:else if kind === 'mouse'}
		<select aria-label="Button" aria-describedby={`${id}-label`} value={arg} onchange={(e) => emit('mouse', (arg = e.currentTarget.value))}>
			<option value="left">Left click</option>
			<option value="right">Right click</option>
			<option value="middle">Middle click</option>
			<option value="back">Back</option>
			<option value="forward">Forward</option>
		</select>
	{:else if kind === 'dpi' || kind === 'dpi-shift'}
		<span class="unit">
			<input
				aria-label="DPI" aria-describedby={`${id}-label`}
				type="number"
				min="200"
				max="8000"
				step="50"
				value={arg}
				onchange={(e) => emitDpi(kind, e.currentTarget)}
			/>
			DPI
		</span>
	{:else if kind === 'dpi-cycle'}
		<input
			aria-label="DPI values, separated by commas" aria-describedby={`${id}-label`}
			value={arg}
			onchange={(e) => emit('dpi-cycle', (arg = e.currentTarget.value.replace(/\s/g, '')))}
		/>
	{:else if kind === 'host'}
		<select aria-label="Computer" aria-describedby={`${id}-label`} value={arg} onchange={(e) => emit('host', (arg = e.currentTarget.value))}>
			<option value="1">Computer 1</option>
			<option value="2">Computer 2</option>
			<option value="3">Computer 3</option>
		</select>
	{:else if kind === 'haptic'}
		<span class="unit">
			<select aria-label="Waveform" aria-describedby={`${id}-label`} value={arg} onchange={(e) => emit('haptic', (arg = e.currentTarget.value))}>
				{#each app.waveforms as w (w)}
					<option value={w}>{waveformName(w)}</option>
				{/each}
			</select>
			<button
				class="cap small"
				disabled={!app.ready}
				onclick={() => app.device && app.attempt(() => api.playHaptic(app.device!.path, arg))}>Feel it</button
			>
		</span>
	{:else if kind === 'shell'}
		<input
			aria-label="Command" aria-describedby={`${id}-label`}
			placeholder="notify-send Hello"
			value={arg}
			onchange={(e) => emit('shell', (arg = e.currentTarget.value))}
		/>
	{/if}
</div>

<style>
	.picker {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}

	input {
		min-width: 0;
		flex: 1 1 10rem;
	}

	input[type='number'] {
		flex: none;
		width: 6.5rem;
	}

	.hold {
		flex-basis: 100%;
		display: grid;
		gap: 0.35rem;
		margin-top: 0.25rem;
		font-size: var(--t-sm);
	}

	.note {
		font-size: var(--t-xs);
		color: var(--muted);
	}

	.unit {
		display: inline-flex;
		gap: 0.5rem;
		align-items: center;
		color: var(--muted);
	}

</style>
