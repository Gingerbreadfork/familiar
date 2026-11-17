<script lang="ts">
	import BindingEditor from './BindingEditor.svelte';
	import Sheet from './Sheet.svelte';
	import Scale from '../ui/Scale.svelte';
	import Switch from '../ui/Switch.svelte';
	import { waveformName } from '../actions';
	import { api } from '../api';
	import { app } from '../state.svelte';

	let s = $derived(app.device?.settings ?? {});
	let offline = $derived(!app.ready);
	let on = $derived(s.haptic !== undefined && s.haptic !== '0');
	let playing = $state<string | null>(null);

	async function play(w: string) {
		const path = app.device?.path;
		if (!path) return;
		playing = w;
		await app.attempt(() => api.playHaptic(path, w), 'haptic');
		setTimeout(() => (playing = playing === w ? null : playing), 600);
	}
</script>

<Sheet id="haptic">
	<section class="group">
		<h2 class="section-title">Pressing it</h2>
		<BindingEditor button="haptic" part="haptic" />
	</section>

	<section class="group">
		<h2 class="section-title">How it feels</h2>
		<Switch
			label="Tap back"
			hint="The panel answers a press, and buttons or scripts can make it tap."
			checked={on}
			disabled={offline}
			onchange={(v) => app.setting('haptic', v ? '60' : '0', 'haptic')}
		/>
		{#if on}
			<Scale
				label="Strength"
				min={1}
				max={100}
				value={Number(s.haptic)}
				disabled={offline}
				ends={['Faint', 'Strong']}
				format={(n) => `${n}%`}
				oncommit={(n) => app.setting('haptic', String(n), 'haptic')}
			/>
		{/if}
		<Scale
			label="Press needed"
			min={3704}
			max={6158}
			step={2}
			value={Number(s.force ?? 4630)}
			disabled={offline}
			ends={['Light touch', 'Firm press']}
			format={(n) => (n === 4630 ? 'As it comes' : String(n))}
			oncommit={(n) => app.setting('force', String(n), 'haptic')}
		/>
	</section>

	<section class="group">
		<h2 class="section-title">Patterns</h2>
		<p class="hint">Press one to feel it under your thumb.</p>
		<div class="waves">
			{#each app.waveforms as w (w)}
				<button class={['cap', 'small', playing === w && 'playing']} disabled={offline || !on} onclick={() => play(w)}>
					{waveformName(w)}
				</button>
			{/each}
		</div>
	</section>
</Sheet>

<style>
	.hint {
		color: var(--muted);
		font-size: var(--t-sm);
		margin-top: -0.6rem;
	}

	.waves {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.playing {
		box-shadow: var(--press);
		transform: translateY(1px);
	}
</style>
