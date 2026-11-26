<script lang="ts">
	import ActionPicker from '../ActionPicker.svelte';
	import Sheet from './Sheet.svelte';
	import Scale from '../ui/Scale.svelte';
	import Switch from '../ui/Switch.svelte';
	import { app } from '../state.svelte';
	import type { ThumbView } from '../types';

	let s = $derived(app.device?.settings ?? {});
	let thumb = $derived(app.config?.thumb ?? null);

	const MODES = [
		{ id: 'system', label: 'Scroll sideways, as it comes' },
		{ id: 'hscroll', label: 'Scroll sideways at a speed you set' },
		{ id: 'volume', label: 'Change the volume' },
		{ id: 'zoom', label: 'Zoom in and out' },
		{ id: 'tabs', label: 'Switch tabs' },
		{ id: 'none', label: 'Nothing' }
	];

	function save(next: ThumbView | null) {
		return app.setThumb(next);
	}

	function pickMode(mode: string) {
		if (mode === 'system') save(null);
		else save({ mode, speed: thumb?.speed ?? 1, tap: thumb?.tap ?? null });
	}

	const times = (n: number) => `${n.toFixed(1)}×`;
</script>

<Sheet id="thumb-wheel">
	<section class="group">
		<label class="field">
			<span>What it does</span>
			<select value={thumb?.mode ?? 'system'} onchange={(e) => pickMode(e.currentTarget.value)}>
				{#each MODES as m (m.id)}
					<option value={m.id}>{m.label}</option>
				{/each}
			</select>
		</label>
		{#if thumb && thumb.mode !== 'none'}
			<Scale
				label="Speed"
				min={0.5}
				max={3}
				step={0.1}
				value={thumb.speed}
				format={times}
				oncommit={(n) => save({ ...thumb, speed: n })}
			/>
		{/if}
		<Switch
			label="Reverse direction"
			checked={s['thumb-invert'] === 'on'}
			disabled={!app.ready}
			onchange={(on) => app.setting('thumb-invert', on ? 'on' : 'off', 'thumb-wheel')}
		/>
	</section>

	{#if thumb}
		<section class="group">
			<h2 class="section-title">Tapping it</h2>
			<p class="hint">A light touch on the wheel without turning it.</p>
			<ActionPicker
				label="Tap the thumb wheel"
				value={thumb.tap}
				onchange={(a) => save({ ...thumb, tap: a === 'none' ? null : a })}
			/>
		</section>
	{/if}
</Sheet>

<style>
	.field {
		display: grid;
		gap: 0.4rem;
	}

	select {
		justify-self: start;
	}

	.hint {
		color: var(--muted);
		font-size: var(--t-sm);
		margin-top: -0.6rem;
	}
</style>
