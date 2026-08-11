<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { Pathname } from '$app/types';
	import { onMount } from 'svelte';
	import { signOut, useSession } from '$lib/auth-client';
	import { api, type ApiUser } from '$lib/api';
	import AppHeader from '$lib/components/app/AppHeader.svelte';

	let { children } = $props();
	const session = useSession();
	let currentUser = $state<ApiUser | null>(null);
	let isSigningOut = $state(false);

	onMount(async () => {
		try {
			currentUser = (await api.getCurrentUser()).user;
		} catch {
			currentUser = null;
		}
	});

	$effect(() => {
		if (!$session.data) currentUser = null;
	});

	$effect(() => {
		if (!$session.isPending && !$session.data) {
			const next = `${page.url.pathname}${page.url.search}`;
			void goto(resolve(('/auth?next=' + encodeURIComponent(next)) as Pathname));
		}
	});

	async function handleSignOut() {
		isSigningOut = true;
		try {
			await signOut();
			await goto(resolve('/'));
		} finally {
			isSigningOut = false;
		}
	}
</script>

{#if $session.isPending}
	<div class="grid min-h-dvh place-items-center" aria-label="Checking your session">
		<div
			class="size-7 animate-spin rounded-full border-2 border-primary border-t-transparent"
		></div>
	</div>
{:else if $session.data}
	<div
		class="min-h-dvh bg-background text-foreground selection:bg-primary selection:text-primary-foreground"
	>
		<a
			class="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:rounded-md focus:bg-primary focus:px-3 focus:py-2 focus:text-primary-foreground"
			href="#main-content">Skip to content</a
		>
		<AppHeader
			user={currentUser}
			{isSigningOut}
			onSignOut={handleSignOut}
			onUserChanged={(user) => (currentUser = user)}
		/>
		{@render children()}
	</div>
{/if}
