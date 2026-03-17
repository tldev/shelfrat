<script lang="ts">
	import { onMount } from 'svelte';

	let dark = $state(false);

	onMount(() => {
		dark = document.documentElement.classList.contains('dark');
	});

	function toggle() {
		dark = !dark;
		document.documentElement.classList.toggle('dark', dark);
		localStorage.setItem('theme', dark ? 'dark' : 'light');
	}
</script>

<button class="theme-toggle" onclick={toggle} title={dark ? 'light mode' : 'dark mode'}>
	{dark ? 'light' : 'dark'}
</button>

<style>
	.theme-toggle {
		font-size: 0.8rem;
		padding: 0.25rem 0.5rem;
		background: transparent;
		color: var(--fg-muted);
		border: none;
	}

	.theme-toggle:hover {
		color: var(--fg);
	}
</style>
