<script lang="ts" module>
	// The threshold to restore when the shift is switched back on.
	let lastShift = '10';
</script>

<script lang="ts">
	import BindingEditor from './BindingEditor.svelte';
	import Sheet from './Sheet.svelte';
	import Rocker from '../ui/Rocker.svelte';
	import Scale from '../ui/Scale.svelte';
	import Switch from '../ui/Switch.svelte';
	import { api } from '../api';
	import { app } from '../state.svelte';

	let s = $derived(app.device?.settings ?? {});
	let offline = $derived(!app.ready);
	let config = $derived(app.config);
	let shifting = $derived(s.smartshift !== undefined && s.smartshift !== 'off');
	const times = (n: number) => `${n.toFixed(1)}×`;
</script>

<Sheet id="wheel">
	<section class="group">
		<Rocker
			label="How it turns"
			value={s.wheel === 'free' ? 'free' : 'ratchet'}
			options={[
				{ value: 'ratchet', label: 'Notch by notch' },
				{ value: 'free', label: 'Free spin' }
			]}
			disabled={offline}
			onchange={(v) => app.setting('wheel', v, 'wheel')}
		/>
		<Switch
			label="Let go when you flick it"
			hint="A quick spin frees the wheel for long pages, then the notches come back."
			checked={shifting}
			disabled={offline}
			onchange={(on) => {
				if (!on && shifting) lastShift = String(s.smartshift);
				app.setting('smartshift', on ? lastShift : 'off', 'wheel');
			}}
		/>
		{#if shifting}
			<Scale
				label="How hard a flick"
				min={1}
				max={Math.max(50, Number(s.smartshift))}
				value={Number(s.smartshift)}
				disabled={offline}
				ends={['Gentle', 'Hard']}
				oncommit={(n) => app.setting('smartshift', String(n), 'wheel')}
			/>
		{/if}
		<Scale
			label="Notch strength"
			min={1}
			max={100}
			value={Number(s.torque ?? 75)}
			disabled={offline}
			ends={['Light', 'Firm']}
			oncommit={(n) => app.setting('torque', String(n), 'wheel')}
		/>
	</section>

	<section class="group">
		<h2 class="section-title">Scrolling</h2>
		<Switch
			label="Smooth scrolling"
			hint="Pages glide in fine steps instead of jumping a line at a time."
			checked={config?.smooth ?? false}
			onchange={(on) => app.attempt(() => api.setScroll(on, config?.speed ?? 1), 'wheel')}
		/>
		{#if config?.smooth}
			<Scale
				label="Speed"
				min={0.5}
				max={3}
				step={0.1}
				value={config.speed}
				format={times}
				oncommit={(n) => app.attempt(() => api.setScroll(true, n), 'wheel')}
			/>
		{/if}
		<Switch
			label="Reverse direction"
			checked={s['wheel-invert'] === 'on'}
			disabled={offline}
			onchange={(on) => app.setting('wheel-invert', on ? 'on' : 'off', 'wheel')}
		/>
	</section>

	<section class="group">
		<h2 class="section-title">Pressing the wheel</h2>
		<BindingEditor button="middle" part="wheel" />
	</section>
</Sheet>
