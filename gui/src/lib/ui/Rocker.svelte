<script lang="ts" generics="T extends string">
	interface Props {
		label: string;
		options: { value: T; label: string }[];
		value: T;
		disabled?: boolean;
		onchange: (v: T) => void;
	}

	let { label, options, value, disabled = false, onchange }: Props = $props();
	const id = $props.id();
</script>

<fieldset class="rocker" {disabled}>
	<legend>{label}</legend>
	<div class="bed">
		{#each options as o (o.value)}
			<label class={['seg', o.value === value && 'on']}>
				<input type="radio" name={id} value={o.value} checked={o.value === value} onchange={() => onchange(o.value)} />
				{o.label}
			</label>
		{/each}
	</div>
</fieldset>

<style>
	fieldset {
		border: 0;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.4rem;
	}

	fieldset:disabled {
		opacity: 0.45;
	}

	legend {
		padding: 0;
		margin-bottom: 0.4rem;
	}

	/* A recessed bed with the chosen position standing up out of it. */
	.bed {
		display: flex;
		padding: 3px;
		gap: 3px;
		border-radius: 9px;
		background: var(--shell);
		box-shadow: var(--press);
	}

	.seg {
		flex: 1;
		position: relative;
		text-align: center;
		padding: 0.45rem 0.7rem;
		border-radius: 7px;
		color: var(--muted);
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}

	.seg.on {
		background: var(--raised);
		color: var(--ink);
		box-shadow: var(--shadow);
		font-weight: 500;
	}

	.seg:has(input:focus-visible) {
		outline: 2px solid var(--ink);
		outline-offset: 1px;
	}

	input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}
</style>
