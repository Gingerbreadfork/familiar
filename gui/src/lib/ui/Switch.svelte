<script lang="ts">
	interface Props {
		label: string;
		checked: boolean;
		hint?: string;
		disabled?: boolean;
		onchange: (on: boolean) => void;
	}

	let { label, checked, hint, disabled = false, onchange }: Props = $props();
</script>

<!-- Drawn after the power switch underneath the mouse: the green shows when it is on. -->
<button class="switch" role="switch" aria-checked={checked} {disabled} onclick={() => onchange(!checked)}>
	<span class="text">
		<span class="label">{label}</span>
		{#if hint}<span class="hint">{hint}</span>{/if}
	</span>
	<span class={['track', checked && 'on']} aria-hidden="true"><span class="knob"></span></span>
</button>

<style>
	.switch {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1.5rem;
		width: 100%;
		padding: 0.2rem 0;
		background: none;
		border: 0;
		text-align: left;
		cursor: pointer;
	}

	.switch:disabled {
		cursor: default;
		opacity: 0.45;
	}

	.text {
		display: grid;
		gap: 0.1rem;
	}

	.hint {
		font-size: var(--t-sm);
		color: var(--muted);
	}

	.track {
		position: relative;
		flex: none;
		width: 46px;
		height: 24px;
		border-radius: 12px;
		background: #2b2c2e;
		box-shadow: inset 0 1px 3px rgb(0 0 0 / 0.5);
		overflow: hidden;
	}

	.track::before {
		content: '';
		position: absolute;
		inset: 3px auto 3px 3px;
		width: 22px;
		border-radius: 9px;
		background: #55585c;
		transition: background 0.2s;
	}

	.track.on::before {
		background: var(--led);
		box-shadow: 0 0 8px var(--led-glow);
	}

	.knob {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: linear-gradient(#fbfbfa, #d9dad7);
		box-shadow: 0 1px 2px rgb(0 0 0 / 0.5);
		transition: transform 0.2s ease;
	}

	.on .knob {
		transform: translateX(22px);
	}
</style>
