<script lang="ts">
	import ActionPicker from '../ActionPicker.svelte';
	import Switch from '../ui/Switch.svelte';
	import { DIAGONALS, GESTURES, NORMALLY, type ButtonId } from '../actions';
	import { app } from '../state.svelte';
	import type { BindingView } from '../types';

	interface Props {
		button: ButtonId;
		/** The part to ripple on the drawing when a change lands. */
		part: string;
	}

	let { button, part }: Props = $props();

	type Mode = 'normal' | 'one' | 'gestures' | 'toggle';

	// Compared as text, so a refresh that changes nothing keeps choices not yet saved.
	let saved = $derived(JSON.stringify(app.config?.buttons[button] ?? null));
	let binding = $derived(JSON.parse(saved) as BindingView | null);
	let gestures = $derived<Record<string, string>>(binding?.kind === 'gestures' ? binding.gestures : {});
	let mode = $derived.by((): Mode => {
		if (!binding) return 'normal';
		if (binding.kind === 'direct') return 'one';
		return binding.gestures.toggle ? 'toggle' : 'gestures';
	});
	let showDiagonals = $state(false);
	let diagonals = $derived(showDiagonals || DIAGONALS.some((d) => d.key in gestures));

	function save(next: BindingView | null) {
		app.setButton(button, next, part);
	}

	function pickMode(m: Mode) {
		mode = m;
		if (m === 'normal') save(null);
	}

	function setGesture(key: string, action: string) {
		const next = { ...gestures };
		delete next.toggle;
		if (action === 'none') delete next[key];
		else next[key] = action;
		save({ kind: 'gestures', gestures: next });
	}

	function setDiagonals(on: boolean) {
		showDiagonals = on;
		if (!on && DIAGONALS.some((d) => d.key in gestures)) {
			const next = { ...gestures };
			for (const d of DIAGONALS) delete next[d.key];
			save({ kind: 'gestures', gestures: next });
		}
	}

	let MODES = $derived<{ id: Mode; label: string; hint: string }[]>([
		{ id: 'normal', label: 'As it comes', hint: NORMALLY[button] },
		{ id: 'one', label: 'One action', hint: 'The same thing every time you press it.' },
		{ id: 'gestures', label: 'Gestures', hint: 'Different actions for a click, a hold, or a hold and swipe.' },
		{ id: 'toggle', label: 'Toggle', hint: 'Click once to hold the action down, click again to let go.' }
	]);
</script>

<fieldset class="modes">
	<legend class="sr">What it does</legend>
	{#each MODES as m (m.id)}
		<label class={['mode', mode === m.id && 'on']}>
			<input type="radio" name={`mode-${button}`} value={m.id} checked={mode === m.id} onchange={() => pickMode(m.id)} />
			<span class="pip" aria-hidden="true"></span>
			<span class="words">
				<span class="name">{m.label}</span>
				<span class="hint">{m.hint}</span>
			</span>
		</label>
	{/each}
</fieldset>

{#if mode === 'one'}
	<div class="edit">
		<ActionPicker
			label="Action"
			placeholder="Choose an action"
			holdable
			value={binding?.kind === 'direct' ? binding.action : null}
			onchange={(action) => save({ kind: 'direct', action })}
		/>
	</div>
{:else if mode === 'toggle'}
	<div class="edit">
		<ActionPicker
			label="Action to hold"
			placeholder="Choose an action"
			value={gestures.toggle ?? null}
			onchange={(action) => save({ kind: 'gestures', gestures: { toggle: action } })}
		/>
	</div>
{:else if mode === 'gestures'}
	<div class="edit rows">
		{#each GESTURES as g (g.key)}
			<div class="row">
				<span class="gesture">{g.label}</span>
				<ActionPicker
					label={g.label}
					holdable={g.key === 'hold'}
					value={gestures[g.key] ?? null}
					onchange={(a) => setGesture(g.key, a)}
				/>
			</div>
		{/each}
		<Switch label="Diagonal swipes too" checked={diagonals} onchange={setDiagonals} />
		{#if diagonals}
			{#each DIAGONALS as g (g.key)}
				<div class="row">
					<span class="gesture">{g.label}</span>
					<ActionPicker label={g.label} value={gestures[g.key] ?? null} onchange={(a) => setGesture(g.key, a)} />
				</div>
			{/each}
		{/if}
	</div>
{/if}

<style>
	fieldset {
		border: 0;
		margin: 0;
		padding: 3px;
		display: grid;
		gap: 2px;
		border-radius: 10px;
		background: var(--shell);
		box-shadow: var(--press);
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}

	.mode {
		position: relative;
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.7rem;
		align-items: start;
		padding: 0.55rem 0.7rem;
		border-radius: 8px;
		cursor: pointer;
	}

	.mode.on {
		background: var(--raised);
		box-shadow: var(--shadow);
	}

	.mode:has(input:focus-visible) {
		outline: 2px solid var(--ink);
		outline-offset: 1px;
	}

	input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}

	/* An indicator light: dark when off, lit when chosen. */
	.pip {
		width: 9px;
		height: 9px;
		margin-top: 0.4rem;
		border-radius: 50%;
		background: var(--line-strong);
		transition:
			background 0.2s,
			box-shadow 0.2s;
	}

	.on .pip {
		background: var(--led);
		box-shadow: 0 0 6px var(--led-glow);
	}

	.words {
		display: grid;
		gap: 0.05rem;
	}

	.name {
		font-weight: 500;
	}

	.hint {
		font-size: var(--t-sm);
		color: var(--muted);
	}

	.edit {
		display: grid;
		gap: 0.8rem;
	}

	.row {
		display: grid;
		gap: 0.3rem;
	}

	.gesture {
		font-size: var(--t-sm);
		color: var(--muted);
	}
</style>
