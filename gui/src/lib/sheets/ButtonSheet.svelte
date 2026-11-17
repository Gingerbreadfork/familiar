<script lang="ts">
	import BindingEditor from './BindingEditor.svelte';
	import Sheet from './Sheet.svelte';
	import type { ButtonId } from '../actions';

	interface Props {
		button: Exclude<ButtonId, 'middle' | 'haptic'>;
	}

	let { button }: Props = $props();
</script>

<Sheet id={button}>
	<section class="group">
		<BindingEditor {button} part={button} />
	</section>
</Sheet>
