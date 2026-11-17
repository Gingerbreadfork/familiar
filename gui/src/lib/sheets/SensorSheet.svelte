<script lang="ts">
	import Sheet from './Sheet.svelte';
	import Scale from '../ui/Scale.svelte';
	import { app } from '../state.svelte';

	let s = $derived(app.device?.settings ?? {});
	let dpi = $derived(Number(s.dpi ?? 1000));
	const PRESETS = [400, 800, 1000, 1600, 2400, 3200, 4000];
</script>

<Sheet id="sensor" title="Pointer speed" where="How far the pointer travels for each inch the mouse moves. Higher is faster.">
	<div class="readout" aria-live="polite">
		<span class="number">{dpi}</span>
		<span class="unit">DPI</span>
	</div>

	<section class="group">
		<Scale
			label="Pointer speed"
			min={200}
			max={8000}
			step={50}
			value={dpi}
			disabled={!app.ready}
			ends={['200', '8000']}
			format={(n) => `${n} DPI`}
			oncommit={(n) => app.setting('dpi', String(n), 'sensor')}
		/>
		<div class="presets" role="group" aria-label="Common speeds">
			{#each PRESETS as p (p)}
				<button
					class={['cap', 'small', dpi === p && 'held']}
					aria-pressed={dpi === p}
					disabled={!app.ready}
					onclick={() => app.setting('dpi', String(p), 'sensor')}>{p}</button
				>
			{/each}
		</div>
	</section>
</Sheet>

<style>
	.readout {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-top: -0.4rem;
	}

	.number {
		font-weight: 300;
		font-size: var(--t-read);
		line-height: 1;
		letter-spacing: -0.02em;
	}

	.unit {
		font-family: var(--narrow);
		font-weight: 500;
		color: var(--muted);
	}

	.presets {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.held {
		box-shadow: var(--press);
		background: var(--shell);
		font-weight: 600;
	}
</style>
