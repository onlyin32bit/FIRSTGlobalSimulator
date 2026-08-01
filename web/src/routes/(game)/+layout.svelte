<script lang="ts">
	import { useSession } from '$lib/auth-client';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	let { children } = $props();

	const session = useSession();

	onMount(() => {
		session.subscribe((s) => {
			if (!s.isPending && !s.data) {
				goto('/auth');
			}
		});
	});
</script>

{#if $session.isPending}
	<div class="h-screen w-full flex items-center justify-center">
		<div class="w-8 h-8 rounded-full border-4 border-primary border-t-transparent animate-spin"></div>
	</div>
{:else if $session.data}
	{@render children()}
{/if}
