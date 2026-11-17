<script lang="ts">
	import Sheet from './Sheet.svelte';
	import { app } from '../state.svelte';

	let d = $derived(app.device);
	let percent = $derived(d?.batteryPercent ?? 0);
	let charging = $derived(app.charging);
	let low = $derived(percent > 0 && percent <= 15);

	function status(): string {
		if (!d?.online) return 'Asleep. Move the mouse to wake it.';
		if (charging) return 'Charging. It keeps working while it charges.';
		if (d.batteryStatus === 'full') return 'Full.';
		if (low) return 'Low. Plug it in at the front with a USB-C cable.';
		return 'Running on battery.';
	}
</script>

<Sheet id="power" title="Battery" where="Charge it with a USB-C cable at the front. The switch underneath turns it off for travel.">
	{#if d?.online && percent}
		<div class="readout" aria-live="polite">
			<span class="number">{percent}</span>
			<span class="unit">%</span>
		</div>
		<div class={['cells', low && 'low', charging && 'charging']} aria-hidden="true">
			{#each { length: 10 }, i (i)}
				<span class={['cell', i < Math.round(percent / 10) && 'full', i === Math.round(percent / 10) - 1 && 'tip']}></span>
			{/each}
		</div>
	{/if}
	<p class="status">{status()}</p>
</Sheet>

<style>
	.readout {
		display: flex;
		align-items: baseline;
		gap: 0.3rem;
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

	.cells {
		display: grid;
		grid-template-columns: repeat(10, 1fr);
		gap: 4px;
		padding: 4px;
		border-radius: 8px;
		background: var(--shell);
		box-shadow: var(--press);
	}

	.cell {
		height: 16px;
		border-radius: 3px;
		background: var(--line);
	}

	.cell.full {
		background: var(--ink);
	}

	.low .cell.full {
		background: var(--danger);
	}

	.charging .tip {
		animation: charge 1.2s ease-in-out infinite alternate;
	}

	@keyframes charge {
		to {
			opacity: 0.3;
		}
	}

	.status {
		color: var(--muted);
	}
</style>
