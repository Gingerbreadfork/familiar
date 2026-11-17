<script lang="ts">
	interface Props {
		label: string;
		min: number;
		max: number;
		step?: number;
		value: number;
		disabled?: boolean;
		/** Words for the two ends, shown under the scale. */
		ends?: [string, string];
		format?: (n: number) => string;
		/** Called once, when the thumb is released. A result of false puts the thumb back. */
		oncommit: (n: number) => unknown;
	}

	let { label, min, max, step = 1, value, disabled = false, ends, format = String, oncommit }: Props = $props();

	const id = $props.id();
	// The number alone, so a refresh that leaves it unchanged does not move the thumb mid-drag.
	let current = $derived(value);
	let live = $derived(current);
	let fill = $derived(Math.min(1, Math.max(0, (live - min) / (max - min))));
	// Detent marks, like the ratchet: one per step when there are few, otherwise about forty.
	let marks = $derived(Math.min(40, Math.round((max - min) / step)));

	async function commit(n: number) {
		if ((await oncommit(n)) === false) live = current;
	}
</script>

<div class={['scale', disabled && 'off']}>
	<div class="head">
		<label for={id}>{label}</label>
		<output for={id}>{format(live)}</output>
	</div>
	<div class="track" style:--fill={fill} style:--marks={marks}>
		<input
			{id}
			type="range"
			{min}
			{max}
			{step}
			{disabled}
			value={live}
			oninput={(e) => (live = Number(e.currentTarget.value))}
			onchange={(e) => commit(Number(e.currentTarget.value))}
		/>
	</div>
	{#if ends}
		<div class="ends" aria-hidden="true"><span>{ends[0]}</span><span>{ends[1]}</span></div>
	{/if}
</div>

<style>
	.scale {
		display: grid;
		gap: 0.3rem;
	}

	.off {
		opacity: 0.45;
	}

	.head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 1rem;
	}

	output {
		font-family: var(--narrow);
		font-weight: 500;
		font-size: var(--t-lead);
	}

	.track {
		position: relative;
		height: 30px;
	}

	/* The ruler: detent marks over the whole length, inked up to the thumb. */
	.track::before,
	.track::after {
		content: '';
		position: absolute;
		left: 7px;
		right: 7px;
		top: 6px;
		height: 18px;
		background:
			repeating-linear-gradient(
				to right,
				currentColor 0 1px,
				transparent 1px calc((100% - 1px) / var(--marks))
			),
			linear-gradient(currentColor, currentColor) 0 50% / 100% 1px no-repeat;
		pointer-events: none;
	}

	.track::before {
		color: var(--line-strong);
		mask: linear-gradient(to bottom, transparent 30%, #000 30% 70%, transparent 70%);
	}

	.track::after {
		color: var(--ink);
		clip-path: inset(0 calc((1 - var(--fill)) * 100%) 0 0);
		mask: linear-gradient(to bottom, transparent 18%, #000 18% 82%, transparent 82%);
	}

	input {
		position: relative;
		z-index: 1;
		appearance: none;
		width: 100%;
		height: 30px;
		margin: 0;
		background: transparent;
		cursor: pointer;
	}

	input:disabled {
		cursor: default;
	}

	input::-webkit-slider-runnable-track {
		height: 30px;
		background: transparent;
	}

	/* A knurled steel cap, like the wheels. */
	input::-webkit-slider-thumb {
		appearance: none;
		width: 14px;
		height: 30px;
		border-radius: 4px;
		border: 1px solid var(--m-steel-4);
		background:
			repeating-linear-gradient(to right, transparent 0 2px, rgb(0 0 0 / 0.22) 2px 3px),
			linear-gradient(to right, var(--m-steel-2), var(--m-steel-1) 40%, var(--m-steel-2) 70%, var(--m-steel-3));
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.3);
	}

	input:focus-visible {
		outline: none;
	}

	input:focus-visible::-webkit-slider-thumb {
		outline: 2px solid var(--ink);
		outline-offset: 2px;
	}

	.ends {
		display: flex;
		justify-content: space-between;
		font-size: var(--t-xs);
		color: var(--muted);
	}
</style>
